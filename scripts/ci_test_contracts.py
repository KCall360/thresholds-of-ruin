"""Reviewed ownership for PR assurance; exhaustive cadence remains independent."""
import ast
from collections import defaultdict
from pathlib import Path, PurePosixPath
import re

SMOKE_LINUX = (
    'test_paired_stairs_process.PairedStairsProcess.test_generated_round_trip_resume_and_spectator_disclosure',
    'test_rogue_exploration_process.RogueExplorationProcess.test_short_journal_only_exploration_resumes_without_source_files',
    'test_discovery_process.DiscoveryProcesses.test_text_examination_and_secret_open_survive_source_free_checkpoint',
    'test_discovery_process.DiscoveryProcesses.test_text_examination_and_secret_open_survive_source_free_journal',
    'physics_smoke_process.PhysicsCorrectnessSmoke.test_all_shapes_preserve_actions_draw_flush_and_resume',
)
SMOKE_WINDOWS = (
    'test_paired_stairs_process.PairedStairsAsciiProcess.test_native_generated_pair_round_trip',
    'test_paired_stairs_process.PairedStairsProcess.test_generated_round_trip_resume_and_spectator_disclosure',
)
SMOKE_RELEASE = (
    'test_checkpoint_process.CheckpointProcesses.test_checkpoint_tail_crash_and_restart_through_real_headless_client',
)
REVIEWED_OWNERS = {
    'physics_smoke_process': ('PhysicsCorrectnessSmoke',),
    'test_adventure_process': ('AdventureProcesses',),
    'test_ai_interactions_process': ('AiInteractionProcesses', 'IdleItemWaitContracts'),
    'test_ai_self_defense_process': ('AiSelfDefenseProcesses',),
    'test_amulet_process': ('AmuletProcesses',),
    'test_arena_evaluation': ('ArenaEvaluationTests', 'ArenaParticipantContractTests', 'ArenaResumeTests', 'AtomicJsonFormatting'),
    'test_arena_evaluation_process': ('ArenaEvaluationProcesses',),
    'test_arena_matrix': ('ArenaMatrixTests', 'BundledRulesTests'),
    'test_arena_matrix_process': ('ArenaMatrixProcesses',),
    'test_arena_matrix_process_harness': ('ArenaMatrixAbilityCoverageTests', 'ArenaMatrixProcessHarnessTests', 'DebugMatrixSample'),
    'test_arena_search': ('ArenaSearchTests',),
    'test_arena_search_parameters': ('MaterializationRecoveryTests', 'SearchParameterTests', 'StableIdentityTests'),
    'test_arena_search_parameters_process': ('SearchParameterProcesses',),
    'test_arena_search_process': ('ArenaSearchProcesses',),
    'test_arena_search_sampling': ('SearchSamplingTests',),
    'test_arena_search_sampling_process': ('SearchSamplingProcesses',),
    'test_arena_starter_study': ('StarterStudyTests',),
    'test_arena_starter_study_process': ('StarterStudyProcesses',),
    'test_ascii_input_completion': ('AsciiInputCompletion',),
    'test_ascii_interactions_process': ('AsciiInteractionProcesses',),
    'test_ascii_memory_process': ('AsciiMemoryProcesses',),
    'test_ascii_messages_process': ('AsciiMessageProcesses', 'HeadlessCombatFixtureContracts'),
    'test_ascii_movement_process': ('AsciiMovementProcesses',),
    'test_ascii_process': ('AsciiHudPickupProcesses', 'AsciiProcesses'),
    'test_asset_presentation_process': ('AssetPresentationProcesses',),
    'test_background_save_process': ('BackgroundSaveProcesses',),
    'test_check_architecture': ('ArchitectureTests', 'FixtureBoundaryTests'),
    'test_checkpoint_payload': ('CheckpointPayload',),
    'test_checkpoint_process': ('CheckpointProcesses', 'NativeCheckpointProcesses'),
    'test_ci_binary_artifact': ('BinaryArtifactTests', 'ContextTests'),
    'test_ci_cost_report': ('CostReport',),
    'test_ci_selected': ('SelectedTests',),
    'test_ci_no_build': ('ToolingNoBuildTests',),
    'test_ci_workflow_contracts': ('WorkflowAdoption',),
    'test_ci_build_key': ('RecipeKeyTests', 'RecipeProcessTests', 'WorkflowKeyTests'),
    'test_runner_failure_diagnostics': ('FailureContexts',),
    'test_ci_observation': ('Observation', 'Probes'),
    'test_ci_python_shards': ('DebugPythonShardContracts', 'PythonShardCoverageTests', 'PythonShardSelectionTests'),
    'test_ci_receipts': ('ReceiptCompleteness',),
    'test_ci_scope': ('GitScopeContract', 'WorkflowScopeContract'),
    'test_ci_test_contracts': ('ContractSelectionTests',),
    'test_ci_test_plan': ('PlanTests',),
    'test_client_responsiveness_process': ('ClientResponsivenessProcesses',),
    'test_combat_diagnostics': ('CombatDiagnosticExports',),
    'test_compiled_inputs_process': ('CompiledInputProcesses',),
    'test_creature_advancement_process': ('CreatureAdvancementProcesses',),
    'test_creature_behavior_process': ('CreatureBehaviorProcesses',),
    'test_creature_inspection_process': ('CombatDiagnosticsProcesses', 'CreatureInspectionProcesses', 'NativeCreatureInspectionProcesses'),
    'test_creature_lifecycle_process': ('CreatureLifecycleProcesses',),
    'test_creature_process': ('CreatureAbilityProcesses', 'NativeCreatureAbilityProcesses'),
    'test_creature_transform_process': ('CreatureTemplateProcesses',),
    'test_diagonal_process': ('DiagonalProcesses',),
    'test_discovery_process': ('DiscoveryProcesses',),
    'test_documentation': ('DocumentationTests',),
    'test_doors_process': ('DoorProcesses',),
    'test_dungeon_process': ('DungeonProcesses',),
    'test_effective_interaction_process': ('EffectiveInteractionProcesses',),
    'test_fall_credit_process': ('FallCreditProcesses',),
    'test_fall_credit_retention': ('FallCreditRetention',),
    'test_feedback': ('ActualCommand', 'Execution', 'Outcomes', 'Planning'),
    'test_finite_claim_report': ('FiniteClaimReports',),
    'test_floor_activation_process': ('FloorActivationProcess', 'OmissionCombatSearchCallerTests', 'OmissionSearchCallerTests', 'OmissionTransportCallerTests', 'OmittedRoomTopologyProcess', 'RogueStarterProcess'),
    'test_geometry_process': ('GeometryProcesses',),
    'test_graphical_waiver': ('GraphicalWaiverContract',),
    'test_grid_scenario_authoring': ('GridScenarioAuthoring',),
    'test_growth_failure_process': ('GrowthFailureProcesses',),
    'test_headless_process': ('HeadlessProcesses',),
    'test_health_recovery_process': ('HealthRecoveryProcesses',),
    'test_intention_process': ('IntentionNativeProcesses', 'IntentionProcesses'),
    'test_interactions_process': ('InteractionProcesses',),
    'test_items_process': ('ItemProcesses',),
    'test_lighting_process': ('LightingProcesses',),
    'test_linked_hold_process': ('LinkedHoldProcesses',),
    'test_living_dungeon_awareness_process': ('LivingDungeonAwarenessProcesses',),
    'test_living_dungeon_control_process': ('GroundScareControlProcesses',),
    'test_living_dungeon_process': ('LivingDungeonProcesses', 'PaidLivingCompletionTests'),
    'test_magic_items_process': ('MagicItemProcesses',),
    'test_material_process': ('MaterialProcesses',),
    'test_measure_bind_wounds': ('BindWoundsRunner', 'BindWoundsValidation'),
    'test_nutrition_process': ('FloorActivationCheckpointHelperTests', 'NutritionAsciiProcesses', 'NutritionProcesses', 'PendingMealSynchronizationTests'),
    'test_nutrition_trigger_process': ('NutritionTriggerNativeProcesses', 'NutritionTriggerProcesses', 'RogueNutritionProcesses'),
    'test_paired_stairs_process': ('PairedStairsAsciiProcess', 'PairedStairsProcess'),
    'test_parallel_tests': ('Durations', 'Execution', 'Planning', 'WorkerFailures'),
    'test_perf_compare': ('CaseParsing', 'Changes', 'ComparisonOwnership', 'Extraction', 'Formatting', 'Pooling', 'Processes', 'Scheduling'),
    'test_perf_ledger': ('CommittedLedger', 'EntryFormat', 'LedgerFormat', 'MachineProbe'),
    'test_performance_driver': ('DriverDeadlines', 'DriverTiming', 'SavedExplorationInput'),
    'test_performance_process': ('NativeWorkloadProcesses', 'PerformanceProcesses', 'StoppedPlaceWorkloadFrames'),
    'test_performance_report': ('CompleteWireValidationTests', 'DiscoveryValidationTests', 'PerformanceReportTests', 'PhaseDWorkTests', 'RogueEvidenceValidation', 'SavedDiscoveryTests', 'StreamingValidationTests'),
    'test_persistent_map_process': ('PersistentMapProcesses',),
    'test_physics_process': ('PhysicsProcesses',),
    'test_physics_smoke_selection': ('BenchmarkProcessSupport', 'PhysicsSmokeSelection'),
    'test_place_hints_process': ('PlaceHintProcesses',),
    'test_player_advancement_process': ('NativeSupplyNavigation', 'PlayerAdvancementNativeProcesses', 'PlayerAdvancementProcesses'),
    'test_population_growth_process': ('PopulationGrowthProcesses',),
    'test_procedural_worlds_process': ('ProceduralWorldProcesses',),
    'test_process_harness': ('ActionCompletion', 'ArtifactRetention', 'BuildTiming', 'CaptureReads', 'CreaturePackages', 'DisclosedExploration', 'DiskOutput', 'DurableFloorReceipts', 'FailureRetention', 'HeadlessDiagnostics', 'NativeDisplayRequirement', 'NativeKeyNames', 'NativeKeyPresses', 'NativeStatsViewport', 'OwnerPolicyPeerCoordination', 'ProcessDeadlines', 'RogueActionSettlement', 'SaveBarrierPublication', 'SaveBarriers'),
    'test_process_module_boundaries': ('ProcessModuleBoundaries',),
    'test_process_work_budget': ('ProcessWorkBudgetTests',),
    'test_projectile_process': ('ProjectileCaptureLaunch', 'ProjectileProcesses'),
    'test_public_plan_package': ('PublicPlanHandoff',),
    'test_public_plan_process': ('PublicPlanNativeProcesses', 'PublicPlanProcesses'),
    'test_record_wire_samples': ('CompleteRecordingContracts', 'ConditionEventSelection', 'DiscoveryRecordingContracts', 'NutritionTriggerFixtureContracts', 'PersistentConditionRecordingContracts', 'SequenceRiderFixtureContracts'),
    'test_requirement_predicates_process': ('RequirementPredicateProcesses',),
    'test_restorable_hit_die_process': ('RestorableHitDieNativeProcesses', 'RestorableHitDieProcesses'),
    'test_restorable_loss_process': ('LossFinishPeerTests', 'RestorableHitDieProcesses', 'RestorableLossAsciiProcesses', 'RestorableLossProcesses'),
    'test_rogue_activation_candidate': ('RogueActivationCandidateTests',),
    'test_rogue_activation_process': ('RogueActivationCandidateProcesses', 'RogueActivationSourceTests'),
    'test_rogue_carrier_process': ('RogueCarrierObserverTests', 'RogueCarrierProcesses'),
    'test_rogue_chill_content': ('RogueChillContent',),
    'test_rogue_chill_process': ('ChillResumePeerTests', 'RogueChillProcesses'),
    'test_rogue_corrosion_ray_content': ('RogueCorrosionRayContent',),
    'test_rogue_corrosion_ray_process': ('RogueCorrosionRayProcesses', 'RogueMechanismSnapshotTests'),
    'test_rogue_discovery_content': ('RogueDiscoveryContent',),
    'test_rogue_earned_route': ('EarnedRouteControllerTests',),
    'test_rogue_earned_route_process': ('RogueEarnedRouteProcesses',),
    'test_rogue_earned_route_process_support': ('CompactRouteSnapshots', 'EarnedRouteEvidenceTests', 'PaidTailBudgetCallerTests', 'RouteResumeHardeningTests'),
    'test_rogue_equipment_content': ('RogueEquipmentContent',),
    'test_rogue_escape_content': ('RogueEscapeContent',),
    'test_rogue_exploration_process': ('RogueBundledPolicy', 'RogueExplorationAsciiProcess', 'RogueExplorationProcess'),
    'test_rogue_living_dungeon_content': ('RogueLivingDungeonContent',),
    'test_rogue_locomotion_content': ('RogueLocomotionContent',),
    'test_rogue_locomotion_process': ('RogueLocomotionProcesses',),
    'test_rogue_magic_receipts': ('RogueControlReceiptTests', 'RogueEquipmentReceiptTests', 'RogueMagicReceiptTests'),
    'test_rogue_perception_content': ('RoguePerceptionContent',),
    'test_rogue_perception_process': ('RoguePerceptionProcesses',),
    'test_rogue_playable_content': ('RoguePlayableContent',),
    'test_rogue_playable_process': ('FixedInitialPopulationFixtureTests', 'RoguePlayableNativeProcesses', 'RoguePlayableProcesses', 'WalkthroughMovementPolicyTests', 'WalkthroughRouteTests', 'WalkthroughTransportTests'),
    'test_rogue_playable_study': ('RoguePlayableStudyTests',),
    'test_rogue_playable_study_process': ('RoguePlayableStudyProcesses',),
    'test_rogue_population_content': ('RoguePopulationContent',),
    'test_rogue_projectile_content': ('RogueProjectileContent',),
    'test_rogue_restoration_content': ('RogueRestorationContent',),
    'test_rogue_restoration_process': ('RogueRestorationProcesses',),
    'test_rogue_roster_content': ('RogueRosterContent',),
    'test_rogue_roster_package': ('RogueRosterPackageTests',),
    'test_rogue_roster_process': ('RogueRosterProcesses',),
    'test_rogue_roster_study': ('RogueRosterStudyTests',),
    'test_rogue_route_archive_cache': ('ArchiveVerificationTests', 'RouteMeasurementTests'),
    'test_rogue_route_budget_process': ('RogueRouteBudgetProcesses',),
    'test_rogue_route_replay': ('RouteReplayTests',),
    'test_rogue_starter_chill_process': ('RogueStarterChillProcesses',),
    'test_rogue_techniques_process': ('RogueTechniqueRouteTests', 'RogueTechniquesProcesses'),
    'test_rogue_test_support': ('CompactAdvancementFrames', 'DisclosedAbilityReceipts', 'DisclosedContactApproach', 'DisclosedFloorRoutes', 'DisclosedStepActions', 'RogueActorChoices', 'RogueCareChoices', 'RogueReachChoices', 'RogueRoomGoldAudit'),
    'test_rogue_theft_process': ('RogueTheftProcesses',),
    'test_rogue_utility_potions_content': ('RogueUtilityPotionsContent',),
    'test_rogue_utility_potions_process': ('RogueUtilityPotionsProcesses',),
    'test_rogue_utility_wands_content': ('RogueUtilityWandsContent',),
    'test_rogue_utility_wands_process': ('RogueUtilityWandsProcesses',),
    'test_rogue_vampire_content': ('RogueVampireContent',),
    'test_rogue_vampire_process': ('RogueVampireProcesses',),
    'test_rogue_wraith_process': ('RogueWraithProcesses', 'RogueWraithTargetSelection'),
    'test_ruleset_process': ('RulesetProcesses',),
    'test_ruleset_sources': ('RulesetSources',),
    'test_run_until_blocked_process': ('LongRunDiagnosticChoices', 'RunUntilBlockedProcesses'),
    'test_saved_exploration_report': ('SavedExplorationReport',),
    'test_scalar_wire': ('ScalarWireTests',),
    'test_scenario_artifact_process': ('ScenarioArtifactProcesses',),
    'test_scenario_packages_process': ('ScenarioPackageProcesses',),
    'test_scenario_references': ('CertificateGuardRegressions', 'ReferenceGuardRegressions', 'ScenarioReferences'),
    'test_scenario_sources_process': ('ScenarioSourceProcesses',),
    'test_segmented_process_capture': ('SegmentedFixtureDirectories', 'SegmentedProcessCapture', 'SegmentedStorageBoundaries'),
    'test_self_ally_targets_process': ('SelfAllyTargetNativeProcesses', 'SelfAllyTargetProcesses'),
    'test_service_contract_ci': ('ServiceContractCargoTests',),
    'test_service_contract_host': ('ServiceHostAdapterTests',),
    'test_shadowcasting_process': ('ShadowcastingProcesses',),
    'test_sight_process': ('SightProcesses',),
    'test_spawn_carried_loot_process': ('SpawnCarriedLootProcesses',),
    'test_starter_catalog': ('StarterCatalogTests',),
    'test_status_condition_process': ('NativeStatusConditionProcesses', 'StatusConditionProcesses', 'StatusPeerContextTests'),
    'test_stream_recovery_process': ('NativeStreamRecoveryProcesses', 'StreamRecoveryProcesses'),
    'test_stream_relay': ('CorruptedItemShapes', 'PressureConnection', 'SharedActorTraffic'),
    'test_streaming_process': ('NativeStreamingProcesses', 'StreamingProcesses'),
    'test_strike_performance_report': ('StrikeReports',),
    'test_strike_sequences_process': ('NativeStrikeSequenceProcesses', 'ResumedSequencePeerTests', 'StrikeSequenceProcesses'),
    'test_structural_performance_report': ('StructuralReports',),
    'test_supply_lifecycle_process': ('SupplyFinishPeerTests', 'SupplyLifecycleProcesses'),
    'test_supply_selection_process': ('SupplySelectionNativeProcesses', 'SupplySelectionProcesses'),
    'test_survival_diagnostics': ('SurvivalDiagnosticReports',),
    'test_survival_diagnostics_process': ('SurvivalDiagnosticsProcesses',),
    'test_survival_process_helpers': ('ObservedMealCompletion', 'SavedCheckpointHandles', 'SavedConditionClocks', 'StreamingSubjectContent', 'SupplySelectionIsolation'),
    'test_targets_process': ('TargetProcesses',),
    'test_test_runner': ('CiShardRunnerContract', 'ImmediateFailureDiagnostics', 'TimedRunnerContract'),
    'test_text_combat_boundaries_process': ('AttackBoundaryTests', 'TextCombatBoundaryProcesses'),
    'test_text_process': ('TextProcesses',),
    'test_timing_correlation': ('CorrelationEvidenceIdentity', 'TimingCorrelation'),
    'test_travel_process': ('TravelJournalProcesses', 'TravelProcesses'),
    'test_verify': ('CiPartitions', 'CiPythonEnvironment', 'CiPythonShardPlanning', 'CiWorkflowContract', 'CrossHeadExecution', 'ExecutionEvidence', 'LocalMouseGate', 'LocalMouseWaiver', 'MissingTests', 'ProcessSelection', 'Reporting', 'Selection', 'ServiceContractPlanning', 'Tiers', 'WorkflowShellSelection'),
    'test_wandering_population_process': ('PaidPopulationCompletionTests', 'WanderingPopulationProcesses'),
    'test_wizard_creature_spawn_process': ('WizardCreatureSpawnProcesses',),
    'test_wizard_process': ('WizardProcesses',),
    'test_workload_reports': ('ClientPerformanceReport', 'CombatReports', 'ExplicitSmokeSelection', 'ItemReports', 'ObservationOwnershipReport', 'PhysicsReports', 'PlacePerformanceReport', 'SceneProjectionCounts', 'SharedHelpers'),
}



# Full cadence runs every discovered test. These IDs are deferred only from the
# unrelated PR baseline, never removed or reported as full qualification.
EXTENDED = {
    'test_rogue_earned_route_process.RogueEarnedRouteProcesses.test_fixed_seed42_earned_hd_reexecutes_with_exact_source_free_restore_and_gross_flows': 'earned-route',
    'test_floor_activation_process.OmittedRoomTopologyProcess.test_ordinary_search_traversal_and_source_free_revisit_keep_real_rooms': 'floor-route',
    'test_floor_activation_process.OmittedRoomTopologyProcess.test_dark_maze_search_traversal_and_source_free_revisit_keep_real_cells': 'floor-route',
    'test_rogue_exploration_process.RogueExplorationProcess.test_headless_exploration_stair_round_trip_resume_and_spectator': 'floor-route',
    'test_rogue_exploration_process.RogueExplorationProcess.test_adventure_uses_generated_floor_stairs': 'floor-route',
    'test_rogue_route_budget_process.RogueRouteBudgetProcesses.test_exact_budget_preserves_paused_paid_work_without_submitting_a_resume': 'earned-route',
    'test_arena_matrix_process.ArenaMatrixProcesses.test_compiler_cli_then_normal_engine_and_replay_for_every_curated_case': 'arena',
    'test_performance_process.PerformanceProcesses.test_physics_benchmark_reports_completed_durable_commit_timings': 'performance',
    'test_performance_process.NativeWorkloadProcesses.test_eight_real_clients_follow_scheduled_turns_and_visibility_changes': 'performance',
    'test_run_until_blocked_process.RunUntilBlockedProcesses.test_a_spectator_that_stops_reading_is_dropped_and_play_continues': 'transport',
    'test_run_until_blocked_process.RunUntilBlockedProcesses.test_small_outbound_byte_budgets_preserve_play_recovery_and_durable_restart': 'transport',
}

# Semantic risk edges. These are deliberate source-owner sets, not fuzzy token
# matching: an unrelated protocol addition must not silently select a long route.
FLOOR_SOURCES = frozenset({
    'crates/server/src/generator.rs', 'crates/server/src/generation_recipe.rs',
    'crates/server/src/generation_features.rs', 'crates/server/src/generation_maze.rs',
    'crates/server/src/scenario_population.rs', 'crates/server/src/scenario_instantiation.rs',
    'crates/server/src/floor_activation_audit.rs', 'crates/simulation/src/floor_activation.rs',
    'crates/simulation/src/discovery.rs', 'crates/simulation/src/perception.rs',
    'crates/simulation/src/travel.rs', 'crates/simulation/src/navigation_map.rs',
    'crates/simulation/src/search_clock.rs', 'crates/simulation/src/awareness.rs',
    'crates/simulation/src/explored_map.rs', 'crates/simulation/src/observation.rs',
    'crates/simulation/src/relocation.rs', 'crates/simulation/src/streaming.rs',
    'crates/server/src/scenario_compiler.rs', 'crates/server/src/scenario_authoring.rs',
    'crates/server/src/scenario_source.rs',
})
GROWTH_SOURCES = frozenset({
    'crates/simulation/src/earned_growth.rs', 'crates/simulation/src/progression.rs',
    'crates/simulation/src/health_recovery.rs', 'crates/simulation/src/recovery_context.rs',
    'crates/simulation/src/combat.rs', 'crates/simulation/src/intention.rs',
    'crates/server/src/save_advancement.rs', 'crates/simulation/src/ai.rs',
    'crates/simulation/src/attacks.rs', 'crates/simulation/src/creature_melee.rs',
    'crates/simulation/src/creature_time.rs', 'crates/simulation/src/conditions.rs',
    'crates/simulation/src/nutrition.rs', 'crates/simulation/src/health.rs',
})
PERSISTENCE_SOURCES = frozenset({
    'crates/server/src/engine.rs', 'crates/server/src/storage.rs', 'crates/server/src/journal.rs',
    'crates/server/src/checkpoint.rs', 'crates/server/src/checkpoint_codec.rs',
    'crates/server/src/save_codec.rs', 'crates/server/src/save_schema.rs',
    'crates/server/src/save_scenario.rs', 'crates/server/src/scenario_package.rs',
    'crates/server/src/scenario_artifact.rs',
    'crates/simulation/src/checkpoint.rs', 'crates/simulation/src/actor_record.rs',
    'crates/simulation/src/lib.rs', 'crates/simulation/src/actions.rs',
    'crates/simulation/src/lifecycle.rs', 'crates/server/src/lib.rs',
    'crates/server/src/actions.rs', 'crates/server/src/action_resolution.rs',
    'crates/server/src/intention.rs', 'crates/server/src/intention_lifecycle.rs',
})
# Wire representations used by route observations/replay, unlike unrelated API additions.
ROUTE_WIRE_PREFIXES = ('crates/protocol/src/wire.rs', 'crates/protocol/src/lib.rs',
                       'crates/protocol/src/creature', 'crates/protocol/src/delta.rs',
                       'crates/protocol/src/codec.rs', 'crates/protocol/src/integers.rs',
                       'crates/protocol/src/bounded.rs', 'crates/protocol/src/recovery.rs',
                       'crates/protocol/src/observation', 'crates/protocol/src/state',
                       'crates/protocol/src/map', 'crates/protocol/src/advancement')
# New protocol modules need an explicit representation/consumer review.
PROTOCOL_SOURCE_OWNERS = frozenset({
    'crates/protocol/src/advancement.rs',
    'crates/protocol/src/attack_view.rs',
    'crates/protocol/src/bounded.rs',
    'crates/protocol/src/codec.rs',
    'crates/protocol/src/combat_diagnostics.rs',
    'crates/protocol/src/combat_trace.rs',
    'crates/protocol/src/creature.rs',
    'crates/protocol/src/creature_catalog.rs',
    'crates/protocol/src/creature_inspection.rs',
    'crates/protocol/src/delta.rs',
    'crates/protocol/src/integers.rs',
    'crates/protocol/src/lib.rs',
    'crates/protocol/src/map_pages.rs',
    'crates/protocol/src/projectile_choice_tests.rs',
    'crates/protocol/src/projectile_choices.rs',
    'crates/protocol/src/projectile_selection.rs',
    'crates/protocol/src/public_plan.rs',
    'crates/protocol/src/recovery.rs',
    'crates/protocol/src/response_kind.rs',
    'crates/protocol/src/strike_inspection.rs',
    'crates/protocol/src/strike_pages.rs',
    'crates/protocol/src/supplies.rs',
    'crates/protocol/src/targets.rs',
    'crates/protocol/src/validation.rs',
    'crates/protocol/src/weapon_preview.rs',
    'crates/protocol/src/wire.rs',
})
CONTROLS = frozenset({'AGENTS.md', 'CONTRIBUTING.md', 'Cargo.toml', 'Cargo.lock',
                      'rust-toolchain.toml', 'rust-toolchain', 'scripts/ci_test_contracts.py',
                      'scripts/ci_test_plan.py', 'scripts/ci_selected.py', 'scripts/verify.py',
                      'scripts/test_runner.py', 'scripts/parallel_tests.py', 'scripts/ci_scope.py',
                      'scripts/ci_binary_artifact.py', 'scripts/ci_no_build.py', 'scripts/service_contract_ci.py',
                      'scripts/ci_build_key.py', 'scripts/ci_python_shards.py',
                      'scripts/graphical_waiver.py', 'scripts/perf_compare.py', 'scripts/arena_search.py'})


# These authoring contracts launch the real tor-scenario compiler without
# inheriting ProcessTestCase. They are not pure tooling. Keep this exception
# list explicit; the planner combines it with independently discovered inheritance.
APPLICATION_OWNERS = frozenset({
    ('test_ruleset_sources', 'RulesetSources'),
    ('test_arena_matrix', 'BundledRulesTests'),
})

OBJECTIVES = {
    'tooling': {'objective': 'Selection, fixture, driver and report integrity', 'cost': 'small', 'evidence': 'deterministic source/negative fixture contracts'},
    'content': {'objective': 'Authored source and unchanged production fixture definitions', 'cost': 'small', 'evidence': 'deterministic source, not runtime acceptance'},
    'application': {'objective': 'Ordinary real server/client behavior', 'cost': 'bounded process', 'evidence': 'actual transport and visible effects'},
    'native': {'objective': 'Genuine native input and presented frames', 'cost': 'desktop process', 'evidence': 'Windows/Linux OS boundary'},
    'persistence': {'objective': 'Save barriers, corruption, exact replay and rejection atomicity', 'cost': 'process/integration', 'evidence': 'real storage and child termination, not hardware power loss'},
    'service': {'objective': 'Production Service and encoded fixture/assertion contracts', 'cost': 'bounded Rust host', 'evidence': 'production host, not pure adapter mocks'},
    'protocol': {'objective': 'Strict encoded messages, private disclosure and client compatibility', 'cost': 'bounded', 'evidence': 'codec plus independent actual consumers'},
    'scenario': {'objective': 'Authoring/load certificates and generated identity', 'cost': 'bounded', 'evidence': 'compiler and source-free ordinary smoke'},
    'floor-route': {'objective': 'Ordinary discovery, complete topology traversal and source-free revisit', 'cost': 'extended', 'evidence': 'fixed ordinary route; not replaced by smoke'},
    'earned-route': {'objective': 'Ordinary earned growth and repeated source-free route equality', 'cost': 'extended', 'evidence': 'fixed acceptance seed, not population/balance evidence'},
    'arena': {'objective': 'All curated compiled arena cases and replay', 'cost': 'extended', 'evidence': 'enumerated seeded cases'},
    'performance': {'objective': 'Maintained producer correctness and repeated workload samples', 'cost': 'extended', 'evidence': 'all-shape smoke vs full repeat distinction'},
    'transport': {'objective': 'Real backpressure, authority and continued ordinary play', 'cost': 'extended', 'evidence': 'OS socket pressure, not Service emulation'},
    'full-cadence': {'objective': 'Every discovered contract on the exhaustive cadence', 'cost': 'full matrix', 'evidence': 'separate qualification, never inferred from PR assurance'},
}


def family(identity, metadata):
    """Classify reviewed owners using independent discovery, not filenames."""
    if identity in EXTENDED:
        return EXTENDED[identity]
    module = identity.split('.')[0]
    if metadata[identity]['graphical']:
        return 'native'
    if metadata[identity]['process']:
        if module in ('test_checkpoint_process', 'test_stream_recovery_process'):
            return 'persistence'
        return 'application'
    if module.endswith('_content'):
        return 'content'
    return 'tooling'


# Reviewed CLI launches and retained source identities: dependency -> consumers.
# This is not a dump of every source string. Unregistered literal references in
# the affected closure select full; a mock/control string may over-select full
# rather than being silently approved as a real invocation.
RUNTIME_SCRIPT_EDGES = {
    'arena_evaluation': ('arena_search', 'arena_starter_study', 'rogue_activation_candidate', 'rogue_roster_study', 'test_arena_evaluation_process', 'test_rogue_playable_study_process'),
    'arena_matrix': ('arena_matrix_process_harness', 'arena_search', 'arena_starter_study', 'rogue_activation_candidate', 'rogue_roster_study'),
    'arena_search': ('test_arena_search_process',),
    'arena_search_parameters': ('arena_search', 'test_arena_search_parameters_process'),
    'arena_search_sampling': ('arena_search', 'test_arena_search_sampling_process'),
    'arena_starter_study': ('test_arena_starter_study_process',),
    'client_performance_report': ('perf_compare',),
    'combat_diagnostics': ('test_combat_diagnostics', 'test_creature_inspection_process'),
    'combat_performance_report': ('perf_compare',),
    'item_performance_report': ('perf_compare',),
    'performance_report': ('perf_compare', 'test_perf_compare'),
    'physics_performance_report': ('perf_compare', 'test_workload_reports'),
    'place_performance_report': ('perf_compare',),
    'process_harness': ('rogue_roster_study',),
    'process_work_budget': ('rogue_earned_route_process',),
    'rogue_activation_oracle': ('rogue_activation_candidate',),
    'rogue_earned_route': ('rogue_earned_route_process',),
    'rogue_roster_study': ('test_rogue_playable_study_process',),
    'ruleset_sources': ('arena_search', 'arena_starter_study', 'rogue_activation_candidate', 'rogue_roster_study'),
    'scalar_wire': ('rogue_activation_candidate', 'rogue_earned_route_process', 'rogue_roster_study'),
    'service_contract_tests': ('service_contract_ci',),
    'strike_performance_report': ('perf_compare', 'test_strike_performance_report'),
    'test_runner': ('parallel_tests', 'test_parallel_tests', 'test_test_runner'),
}


def _callers(root, changed_modules=()):
    """Transitive local Python imports, including function-local imports.

    Dynamic/resource edges are handled explicitly below or fall back to full.
    Parsing is source-only: never import an application or run test setup here.
    """
    files = {path.stem: path for path in (root / 'scripts').glob('*.py')}
    reverse = defaultdict(set)
    references = set()
    for module, path in files.items():
        for node in ast.walk(ast.parse(path.read_text(encoding='utf-8-sig'))):
            if (isinstance(node, ast.Constant) and isinstance(node.value, str)
                    and '\n' not in node.value and node.value.endswith('.py')):
                dependency = PurePosixPath(node.value).stem
                if dependency in files and dependency != module:
                    references.add((dependency, module))
            names = ([item.name for item in node.names] if isinstance(node, ast.Import)
                     else [node.module or ''] if isinstance(node, ast.ImportFrom) else [])
            for name in names:
                dependency = name.split('.')[0]
                if dependency in files:
                    reverse[dependency].add(module)
    for dependency, callers in RUNTIME_SCRIPT_EDGES.items():
        if dependency in files:
            reverse[dependency].update(caller for caller in callers if caller in files)
    missing = sorted((dependency, caller) for dependency, caller in references
                     if caller not in reverse[dependency])
    affected = set().union(*(_dependents(module, reverse) for module in changed_modules)) if changed_modules else set()
    blocking = [(dependency, caller) for dependency, caller in missing
                if dependency in affected or caller in affected]
    if blocking:
        raise ValueError('unreviewed local-script reference: ' + repr(blocking[:5]))
    return reverse


def _dependents(module, reverse):
    found, pending = {module}, [module]
    while pending:
        for caller in reverse[pending.pop()] - found:
            found.add(caller)
            pending.append(caller)
    return found


def select(paths, discovered_ids, *, root, test_metadata=None):
    """Return exact PR Python owners; any ambiguity requests exhaustive CI.

    Caller supplies independently discovered current IDs including explicit
    physics smoke. New methods in a reviewed class are selected with their
    changed module; unknown module/classes and missing boundary IDs fail closed.
    Rust/Service compilation/tests and 3D renderer jobs are separate owners.
    """
    result = dict(full_required=False, requires_rust=False, requires_apps=False, reasons=[], contracts=['full-cadence'],
                  linux_debug=[], windows_debug=[], linux_release=[], consumer_3d=False)

    def full(reason):
        result['full_required'] = True
        result['reasons'].append(reason)

    if (not isinstance(discovered_ids, list) or not discovered_ids
            or any(not isinstance(value, str) or not re.fullmatch(r'[A-Za-z_]\w*\.[A-Za-z_]\w*\.test_\w+', value)
                   for value in discovered_ids)):
        full('missing or malformed current discovery')
        return result
    current = set(discovered_ids)
    if (not isinstance(test_metadata, dict) or set(test_metadata) != current
            or any(not isinstance(row, dict) or set(row) != {'process', 'graphical'}
                   or type(row['process']) is not bool or type(row['graphical']) is not bool
                   or row['graphical'] and not row['process']
                   for row in test_metadata.values())):
        full('missing or inconsistent independent test metadata')
    if not result['full_required'] and any(
            tuple(identity.split('.')[:2]) in APPLICATION_OWNERS and not test_metadata[identity]['process']
            for identity in current):
        full('compiler-requiring owner misclassified as pure tooling')
    if len(current) != len(discovered_ids):
        full('duplicate discovered identity')
    for identity in sorted(current):
        module, cls, _ = identity.split('.')
        if cls not in REVIEWED_OWNERS.get(module, ()):
            full(f'unreviewed owner: {module}.{cls}')
    if not set(SMOKE_LINUX + SMOKE_WINDOWS + SMOKE_RELEASE) <= current:
        full('required fixed boundary witness is missing')
    elif not result['full_required'] and (
            any(not test_metadata[identity]['process'] for identity in SMOKE_LINUX + SMOKE_WINDOWS + SMOKE_RELEASE)
            or not test_metadata[SMOKE_WINDOWS[0]]['graphical']):
        full('fixed application/native witness lost its discovery boundary')
    if not isinstance(paths, list) or not paths:
        full('missing complete changed paths')
    if result['full_required']:
        result['reasons'] = sorted(set(result['reasons']))
        return result
    root = Path(root)
    modules = defaultdict(set)
    for identity in current:
        modules[identity.split('.')[0]].add(identity)
    process = {identity for identity in current if test_metadata[identity]['process']}
    ordinary = process - EXTENDED.keys()
    linux = (current - process) | set(SMOKE_LINUX)
    windows, release = (current - process) | set(SMOKE_WINDOWS), set(SMOKE_RELEASE)
    contracts = {'tooling', 'application', 'persistence', 'scenario', 'full-cadence'}
    reverse = None

    def add_domain(domain):
        contracts.add(domain)
        linux.update(identity for identity in current if EXTENDED.get(identity) == domain)

    for path in paths:
        if (not isinstance(path, str) or not path or '\\' in path or path.startswith('/')
                or any(part in ('', '.', '..') for part in path.split('/'))):
            full('unsafe or ambiguous changed path')
            continue
        if not (root / path).is_file():
            full(f'deleted or unavailable source: {path}')
            continue
        if path in CONTROLS or path.startswith('.github/') or path.endswith('/Cargo.toml') or path.endswith('/Cargo.lock'):
            full(f'verification/build controls changed: {path}')
            continue
        if path.startswith('scripts/') and path.endswith('.py') and len(PurePosixPath(path).parts) == 2:
            try:
                if reverse is None:
                    reverse = _callers(root, [Path(value).stem for value in paths
                                             if isinstance(value, str) and value.startswith('scripts/') and value.endswith('.py')])
                closure = _dependents(Path(path).stem, reverse)
                affected = closure & modules.keys()
                if 'service_contract_tests' in closure:
                    result['requires_rust'] = result['requires_apps'] = True
                    contracts.add('service')
            except (OSError, SyntaxError, UnicodeError, ValueError):
                full('cannot resolve Python source dependencies')
                continue
            if not affected:
                full(f'unowned or dynamic Python dependency: {path}')
                continue
            selected = set().union(*(modules[name] for name in affected))
            result['requires_apps'] |= bool(selected & process)
            linux.update(selected)
            windows.update(selected)
            contracts.update(family(identity, test_metadata) for identity in selected)
            if path in ('scripts/process_harness.py', 'scripts/segmented_process_capture.py', 'scripts/process_work_budget.py'):
                # Runtime fixture/class machinery has dependencies beyond imports.
                linux.update(process)
                windows.update(process)
                contracts.update(EXTENDED.values())
            continue
        if path.startswith('scripts/'):
            # Runtime open()/glob()/compiled fixture paths are not import edges.
            # Keep conservative until an explicit reviewed data owner is added.
            full(f'unmapped dynamic script data: {path}')
            continue
        if path.startswith('crates/client-3d/scenarios/') or path.startswith('scenarios/'):
            result['requires_rust'] = result['requires_apps'] = True
            linux.update(ordinary)
            contracts.add('scenario')
            if path.startswith('scenarios/rogue-exploration/'):
                add_domain('floor-route')
                add_domain('earned-route')
            continue
        if path.startswith('crates/client-3d/'):
            result['consumer_3d'] = True
            contracts.add('native')
            continue
        parts = path.split('/')
        if len(parts) >= 3 and parts[0] == 'crates' and parts[1] in (
                'world', 'simulation', 'server', 'protocol', 'client-common',
                'client-text', 'client-ascii', 'client-headless', 'test-support'):
            crate = parts[1]
            if path.startswith('crates/protocol/src/') and path not in PROTOCOL_SOURCE_OWNERS:
                full(f'unreviewed protocol source owner: {path}')
                continue
            result['requires_rust'] = result['requires_apps'] = True
            linux.update(ordinary)
            contracts.add('application')
            # Shared simulation/world mechanics reach both public navigation and
            # earned growth. Broad ownership is safer than a partial symbol graph.
            if path.startswith(('crates/simulation/src/', 'crates/world/src/')):
                add_domain('floor-route')
                add_domain('earned-route')
                if crate == 'simulation':
                    add_domain('arena')
            if crate in ('protocol', 'client-common'):
                result['consumer_3d'] = True
                contracts.add('protocol')
            if crate in ('server', 'protocol', 'client-common', 'client-ascii'):
                windows.update(ordinary)
            if path in FLOOR_SOURCES or path.startswith(('crates/world/src/', 'crates/simulation/src/perception/')):
                add_domain('floor-route')
            if path in GROWTH_SOURCES or path.startswith(('crates/simulation/src/combat/', 'crates/simulation/src/creatures/', 'crates/simulation/src/intention/', 'crates/simulation/src/attacks/', 'crates/simulation/src/nutrition/')):
                add_domain('earned-route')
            if path in PERSISTENCE_SOURCES or path.startswith(ROUTE_WIRE_PREFIXES):
                add_domain('floor-route')
                add_domain('earned-route')
                contracts.add('persistence')
                release.update(identity for identity in ordinary if identity.startswith(('test_checkpoint_process.', 'test_stream_recovery_process.')))
            if path.startswith(('crates/server/src/host', 'crates/server/src/service', 'crates/server/src/transport', 'crates/server/src/outbound', 'crates/server/src/session', 'crates/server/src/runner', 'crates/server/src/main.rs')):
                add_domain('transport')
            if path.startswith('crates/server/examples/'):
                add_domain('performance')
                release.update(identity for identity in current if EXTENDED.get(identity) == 'performance')
            if path.startswith(('crates/server/src/arena', 'crates/simulation/data/')):
                add_domain('arena')
            if path.startswith('crates/simulation/data/'):
                add_domain('floor-route')
                add_domain('earned-route')
            continue
        if path.startswith('docs/') and path.endswith('.md') or path == 'README.md':
            if path == 'docs/reference/protocol.md':
                contracts.add('protocol')
                result['consumer_3d'] = True
                result['requires_rust'] = result['requires_apps'] = True
            continue
        full(f'unmapped source or data: {path}')

    if not result['requires_apps']:
        linux.difference_update(process)
        windows.difference_update(process)
        release.clear()
        contracts.difference_update({'application', 'persistence', 'scenario'})
    result.update(linux_debug=sorted(linux), windows_debug=sorted(windows), linux_release=sorted(release),
                  contracts=sorted(contracts), reasons=sorted(set(result['reasons'])))
    return result
