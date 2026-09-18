use futures_util::{SinkExt, StreamExt};
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, Command as ProcessCommand, Stdio};
use std::sync::mpsc;
use std::time::Duration;
use tokio::time::timeout;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tor_protocol::*;

struct ChildGuard(Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn launch(path: &Path) -> (ChildGuard, String) {
    let mut child = ChildGuard(
        ProcessCommand::new(env!("CARGO_BIN_EXE_tor-server"))
            .args(["--listen", "127.0.0.1:0", "--seed", "42", "--save"])
            .arg(path)
            .env_remove("TOR_SPECTATOR_TOKEN")
            .env("TOR_SERVER_TOKEN", "process-test-token-not-a-real-secret")
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap(),
    );
    let stdout = child.0.stdout.take().unwrap();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut line = String::new();
        let result = BufReader::new(stdout).read_line(&mut line).map(|_| line);
        let _ = tx.send(result);
    });
    let ready = rx
        .recv_timeout(Duration::from_secs(10))
        .expect("server ready deadline")
        .unwrap();
    assert!(!ready.contains("process-test-token"));
    let ready: serde_json::Value = serde_json::from_str(&ready).unwrap();
    let address = format!("ws://{}", ready["address"].as_str().unwrap());
    (child, address)
}

#[tokio::test]
async fn actual_server_process_persists_an_action_and_annotation_across_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("game.json");
    let (child, address) = launch(&path);
    let (mut socket, _) = timeout(Duration::from_secs(5), connect_async(&address))
        .await
        .unwrap()
        .unwrap();
    let hello = ClientMessage::Hello {
        protocol: PROTOCOL_VERSION,
        token: "process-test-token-not-a-real-secret".into(),
        frontend: "test-text".into(),
    };
    socket
        .send(Message::Text(serde_json::to_string(&hello).unwrap().into()))
        .await
        .unwrap();
    timeout(Duration::from_secs(5), socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let attach = ClientMessage::Request {
        request_id: "attach".into(),
        request: Request::Attach { actor: ActorId(1) },
    };
    socket
        .send(Message::Text(
            serde_json::to_string(&attach).unwrap().into(),
        ))
        .await
        .unwrap();
    let frame = timeout(Duration::from_secs(5), socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let ServerMessage::Snapshot { snapshot, .. } =
        serde_json::from_str(frame.to_text().unwrap()).unwrap()
    else {
        panic!("snapshot required")
    };
    let branch = snapshot.branch;
    let commands = [
        ("acquire", Request::AcquireControl),
        (
            "wait",
            Request::Command {
                branch: branch.clone(),
                command: Command::Act {
                    expected_revision: 0,
                    action: Action::Wait,
                },
            },
        ),
        (
            "note",
            Request::Command {
                branch: branch.clone(),
                command: Command::Annotate {
                    anchor: Anchor::State { revision: 1 },
                    text: "Remember this after restarting.".into(),
                    source: ClientSource::User,
                    audience: Audience::Private,
                    category: AnnotationCategory::Note,
                },
            },
        ),
    ];
    for (id, request) in commands {
        let request = ClientMessage::Request {
            request_id: id.into(),
            request,
        };
        socket
            .send(Message::Text(
                serde_json::to_string(&request).unwrap().into(),
            ))
            .await
            .unwrap();
        loop {
            let frame = timeout(Duration::from_secs(5), socket.next())
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            match serde_json::from_str::<ServerMessage>(frame.to_text().unwrap()).unwrap() {
                ServerMessage::Ack { request_id, .. } if request_id == id => break,
                ServerMessage::Update { .. } => {}
                other => panic!("{other:?}"),
            }
        }
    }
    drop(socket);
    drop(child); // Simulates process termination, not a graceful save command.
    let (_resumed, address) = launch(&path);
    let (mut socket, _) = timeout(Duration::from_secs(5), connect_async(&address))
        .await
        .unwrap()
        .unwrap();
    socket
        .send(Message::Text(serde_json::to_string(&hello).unwrap().into()))
        .await
        .unwrap();
    timeout(Duration::from_secs(5), socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    socket
        .send(Message::Text(
            serde_json::to_string(&attach).unwrap().into(),
        ))
        .await
        .unwrap();
    let frame = timeout(Duration::from_secs(5), socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let ServerMessage::Snapshot { snapshot, .. } =
        serde_json::from_str(frame.to_text().unwrap()).unwrap()
    else {
        panic!("snapshot required")
    };
    assert_eq!(snapshot.branch, branch);
    assert_eq!(snapshot.state.observation.tick, 100);
    assert_eq!(snapshot.state.revision, 1);
    assert_eq!(snapshot.history.entries.len(), 2);
    assert!(
        matches!(&snapshot.history.entries[1].content, HistoryContent::Annotation { text, .. } if text == "Remember this after restarting.")
    );
    // A lost acknowledgement can be recovered after restart without reacquiring
    // control. The original action must not execute for a second time.
    assert!(!snapshot.has_control);
    let retry = ClientMessage::Request {
        request_id: "wait".into(),
        request: Request::Command {
            branch,
            command: Command::Act {
                expected_revision: 0,
                action: Action::Wait,
            },
        },
    };
    socket
        .send(Message::Text(serde_json::to_string(&retry).unwrap().into()))
        .await
        .unwrap();
    let frame = timeout(Duration::from_secs(5), socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(
        serde_json::from_str::<ServerMessage>(frame.to_text().unwrap()).unwrap(),
        ServerMessage::Ack {
            request_id: "wait".into(),
            entry_id: Some(snapshot.history.entries[0].id.clone()),
        }
    );
}
