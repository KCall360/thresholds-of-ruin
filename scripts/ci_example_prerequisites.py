"""Exact example-build prerequisites for independently selected process owners.

This is compilation scheduling, never test selection or test-pass evidence.
Each owner still performs its Cargo freshness check and actual bounded runtime.
"""

OWNERS = {
    'test_performance_process.PerformanceProcesses.test_small_maintained_producers_emit_complete_valid_reports': (
        'combat_bench', 'item_bench', 'place_bench', 'scenario_bench', 'horizon-profile'),
    'test_performance_process.PerformanceProcesses.test_rogue_benchmark_preserves_floor_work_and_residency_bounds': ('rogue_bench',),
    'test_performance_process.PerformanceProcesses.test_physics_benchmark_reports_completed_durable_commit_timings': ('physics_bench',),
    'test_performance_process.PerformanceProcesses.test_streaming_benchmark_emits_valid_nested_acquisition_profiles': ('latency_bench',),
    'physics_smoke_process.PhysicsCorrectnessSmoke.test_all_shapes_preserve_actions_draw_flush_and_resume': ('physics_bench',),
}


def stage(test_ids, profile):
    """Return the one exact build stage, or none for non-example owners."""
    if profile not in ('debug', 'release'):
        raise ValueError('Invalid example prerequisite profile')
    if not isinstance(test_ids, (list, tuple)) or not all(isinstance(name, str) for name in test_ids):
        raise ValueError('Example prerequisites require explicit test identities')
    examples = sorted({example for name in test_ids for example in OWNERS.get(name, ())})
    if not examples:
        return None
    command = ['cargo', 'build', '-p', 'tor-server', '--locked']
    for example in examples:
        command.extend(['--example', example])
    if profile == 'release':
        command.append('--release')
    return (f'build-examples-{profile}', command, {})
