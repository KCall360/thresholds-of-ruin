use super::*;
use tor_simulation::RecordStore;
use tor_world::{RegionId, Shared};

#[test]
fn detached_historical_source_validation_is_lazy_source_free_and_atomic_at_materialization() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("source");
    let source = chart_source(&root, "detect_magic");
    let content = source.content().unwrap();
    let mut wire = Wire::new(Engine::open(temp.path().join("saved.db"), source).unwrap());
    let dose = potion(&mut wire);
    wire.act("acquire-before-detach", Action::Drink { item: dose });
    wire.snapshot(OWNER);
    wire.service.engine.flush().unwrap();
    let crate::engine::DetachedChartTestFixture {
        game: detached,
        records,
        package,
    } = wire.service.engine.test_detach_chart_records();
    assert_eq!(records.len(), 1);
    let (id, valid) = records[0].clone();
    assert_eq!(valid.historical_chart_authorities().count(), 1);
    let encoded = serde_json::to_value(&*valid).unwrap();
    let valid: tor_simulation::RegionRecordData = serde_json::from_value(encoded.clone()).unwrap();
    let valid = Shared::new(valid.restore(content.clone()).unwrap());
    // Retained package input, not authored paths, supplies canonical metadata.
    std::fs::remove_dir_all(&root).unwrap();
    let mut regions = crate::regions::Regions::new(
        package.clone(),
        42,
        crate::Streaming {
            active_radius: 0,
            load_radius: 0,
        },
    )
    .unwrap();
    regions.put(id, valid.clone());
    assert_eq!(regions.get(id).unwrap(), valid);
    assert_eq!(regions.test_pending_get(id, valid.clone()).unwrap(), valid);
    let target = tor_simulation::RegionTransition {
        active: BTreeSet::from([RegionId(1)]),
        loaded: BTreeSet::from([RegionId(1)]),
    };
    let mut positive = detached.clone();
    positive
        .apply_region_transition(&target, &mut regions)
        .unwrap();
    assert!(positive
        .chart_history(tor_simulation::ActorId(OWNER.0))
        .unwrap()
        .get(tor_simulation::chart_history::Kind::Magic)
        .latest()
        .is_some());

    let mut forged = encoded;
    // LocalHistory uses the existing compact checkpoint-world codec, not a
    // JSON array: geometry is pooled separately from door-state instances.
    // Decode/re-encode through that codec rather than guessing its layout.
    let mut worlds =
        tor_world::checkpoint_worlds::deserialize(forged["chart_history"]["worlds"].clone())
            .unwrap();
    assert_eq!(worlds.len(), 1, "one acquired historical World");
    let original_bounds = worlds[0].region(RegionId(1)).unwrap().bounds;
    let mut captured = serde_json::to_value(&worlds[0]).unwrap();
    let width = captured
        .pointer_mut("/regions/1/bounds/width")
        .expect("standalone World region extent");
    *width = json!(width.as_i64().expect("saved historical extent") + 1);
    worlds[0] = serde_json::from_value(captured).unwrap();
    assert!(worlds[0].historical_checkpoint_valid());
    assert_eq!(
        worlds[0].region(RegionId(1)).unwrap().bounds.dimensions(),
        (
            original_bounds.dimensions().0 + 1,
            original_bounds.dimensions().1,
            original_bounds.dimensions().2,
        )
    );
    let pooled =
        tor_world::checkpoint_worlds::serialize(&worlds, serde_json::value::Serializer).unwrap();
    assert_eq!(
        tor_world::checkpoint_worlds::deserialize(pooled.clone()).unwrap(),
        worlds,
        "forged extent survives the actual compact-world codec"
    );
    forged["chart_history"]["worlds"] = pooled;
    let bad: tor_simulation::RegionRecordData = serde_json::from_value(forged).unwrap();
    let bad = Shared::new(
        bad.restore(content)
            .expect("structurally lawful changed historical World requires source audit"),
    );
    assert_eq!(bad.historical_chart_authorities().count(), 1);
    // Same record ID replacement must not inherit an earlier successful proof.
    regions.put(id, bad.clone());
    assert!(regions.get(id).is_none());
    assert!(regions.test_pending_get(id, bad.clone()).is_none());
    let before_reads = regions.reads;
    let mut rejected = detached.clone();
    assert!(rejected
        .apply_region_transition(&target, &mut regions)
        .is_err());
    assert_eq!(
        rejected, detached,
        "invalid detached history never attached/published"
    );
    assert_eq!(regions.reads, before_reads);
    let store = wire.service.engine.flush_handle().unwrap();
    let conn = rusqlite::Connection::open(temp.path().join("saved.db")).unwrap();
    for prepared in [false, true] {
        // Actual bounded CRC/framing/content decode precedes lazy source audit.
        let valid_frame = crate::storage::encode_region(id.0, &valid).unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO regions(record,frame) VALUES (?1,?2)",
            rusqlite::params![id.0 as i64, valid_frame],
        )
        .unwrap();
        let mut disk = crate::regions::Regions::new(
            package.clone(),
            42,
            crate::Streaming {
                active_radius: 0,
                load_radius: 0,
            },
        )
        .unwrap();
        disk.attach_disk(store.clone());
        if prepared {
            disk.test_prepare_record_read(id);
        }
        assert_eq!(disk.get(id).unwrap(), valid);
        assert_eq!(disk.reads, 1);
        assert_eq!(disk.prepared, usize::from(prepared));
        disk.stop_preloading();
        let bad_frame = crate::storage::encode_region(id.0, &bad).unwrap();
        conn.execute(
            "UPDATE regions SET frame=?1 WHERE record=?2",
            rusqlite::params![bad_frame, id.0 as i64],
        )
        .unwrap();
        if prepared {
            disk.test_prepare_record_read(id);
        }
        let reads = disk.reads;
        let prepared_count = disk.prepared;
        assert!(disk.get(id).is_none());
        assert_eq!((disk.reads, disk.prepared), (reads, prepared_count));
        let mut candidate = detached.clone();
        assert!(candidate
            .apply_region_transition(&target, &mut disk)
            .is_err());
        assert_eq!(candidate, detached);
    }
}
