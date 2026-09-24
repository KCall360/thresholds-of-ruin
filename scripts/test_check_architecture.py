"""Behavior tests for the workspace dependency boundary check."""

import unittest

from check_architecture import violations


def metadata(packages):
    return {"packages": packages, "workspace_members": [p["id"] for p in packages]}


def package(name, dependencies=()):
    return {"name": name, "id": name, "dependencies": list(dependencies)}


def dependency(name, **options):
    return {"name": name, "path": "/workspace/" + name, **options}


class ArchitectureTests(unittest.TestCase):
    def test_server_dev_edges_are_explicit_and_never_runtime_edges(self):
        for target in ("tor-client-common", "tor-client-ascii", "tor-test-support"):
            for kind in (None, "build", "dev"):
                graph = metadata([package("tor-server", [dependency(target, kind=kind)]), package(target)])
                self.assertEqual(violations(graph), [] if kind == "dev" else [f"tor-server -> {target} is forbidden"])
        graph = metadata([package("tor-server", [dependency("outside-helper", kind="dev")])])
        self.assertEqual(violations(graph), ["tor-server -> outside-helper is forbidden"])

    def test_clients_can_use_shared_protocol(self):
        graph = metadata([
            package("tor-client-text", [dependency("tor-client-common")]),
            package("tor-client-common", [dependency("tor-protocol")]),
            package("tor-protocol"),
        ])
        self.assertEqual(violations(graph), [])

    def test_backend_cannot_leak_through_client_common(self):
        graph = metadata([
            package("tor-client-ascii", [dependency("tor-client-common")]),
            package("tor-client-common", [dependency("tor-simulation")]),
            package("tor-simulation"),
        ])
        self.assertEqual(violations(graph), ["tor-client-common -> tor-simulation is forbidden"])

    def test_alias_optional_target_and_dev_flags_do_not_hide_an_edge(self):
        for options in [
            {"rename": "innocent_alias"},
            {"optional": True},
            {"target": "cfg(windows)"},
            {"kind": "dev"},
            {"kind": "build"},
        ]:
            with self.subTest(options=options):
                graph = metadata([
                    package("tor-client-text", [dependency("tor-world", **options)]),
                    package("tor-world"),
                ])
                self.assertEqual(violations(graph), ["tor-client-text -> tor-world is forbidden"])

    def test_new_workspace_crate_requires_an_explicit_policy(self):
        self.assertEqual(
            violations(metadata([package("tor-new-helper")])),
            ["tor-new-helper has no dependency policy"],
        )

    def test_headless_client_cannot_import_authoritative_state(self):
        for backend in ("tor-world", "tor-simulation", "tor-server"):
            graph = metadata([
                package("tor-client-headless", [dependency(backend)]),
                package(backend),
            ])
            self.assertEqual(violations(graph), [f"tor-client-headless -> {backend} is forbidden"])

    def test_unreviewed_local_dependency_is_rejected_but_registry_crates_are_not(self):
        graph = metadata([
            package("tor-protocol", [
                dependency("outside-helper"),
                {"name": "serde", "source": "registry+example"},
            ]),
        ])
        self.assertEqual(violations(graph), ["tor-protocol -> outside-helper is forbidden"])


if __name__ == "__main__":
    unittest.main()
