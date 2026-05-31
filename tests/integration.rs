//! Integration tests for lau-construct-integration

use lau_construct_integration::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tempfile::TempDir;
use uuid::Uuid;

// ═══════════════════════════════════════════════════════════════════════════════
// 1. Shell kernel + tile store
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn shell_bootstrap_creates_db() {
    let dir = TempDir::new().unwrap();
    let kernel = ShellKernel::bootstrap(dir.path()).unwrap();
    assert_eq!(kernel.tile_count().unwrap(), 0);
}

#[test]
fn shell_create_room() {
    let dir = TempDir::new().unwrap();
    let mut kernel = ShellKernel::bootstrap(dir.path()).unwrap();
    kernel
        .create_room(&RoomConfig {
            name: "bridge".into(),
            gravity: 1.0,
            capacity: 50,
        })
        .unwrap();
    let fetched = kernel.get_room("bridge").unwrap().unwrap();
    assert_eq!(fetched.name, "bridge");
    assert_eq!(fetched.gravity, 1.0);
}

#[test]
fn shell_store_and_query_tile() {
    let dir = TempDir::new().unwrap();
    let mut kernel = ShellKernel::bootstrap(dir.path()).unwrap();
    kernel
        .create_room(&RoomConfig {
            name: "lab".into(),
            gravity: 0.5,
            capacity: 100,
        })
        .unwrap();
    let tile = Tile {
        id: Uuid::new_v4().to_string(),
        room: "lab".into(),
        content: "experiment data".into(),
        gravity: 0.5,
        created_at: 1000,
    };
    kernel.store_tile(&tile).unwrap();
    let tiles = kernel.query_tiles("lab").unwrap();
    assert_eq!(tiles.len(), 1);
    assert_eq!(tiles[0].content, "experiment data");
}

#[test]
fn shell_multiple_rooms() {
    let dir = TempDir::new().unwrap();
    let mut kernel = ShellKernel::bootstrap(dir.path()).unwrap();
    for name in &["alpha", "beta", "gamma"] {
        kernel
            .create_room(&RoomConfig {
                name: (*name).into(),
                gravity: 1.0,
                capacity: 10,
            })
            .unwrap();
    }
    for name in &["alpha", "beta", "gamma"] {
        assert!(kernel.get_room(name).unwrap().is_some());
    }
}

#[test]
fn shell_tiles_isolated_by_room() {
    let dir = TempDir::new().unwrap();
    let mut kernel = ShellKernel::bootstrap(dir.path()).unwrap();
    kernel
        .create_room(&RoomConfig {
            name: "r1".into(),
            gravity: 1.0,
            capacity: 10,
        })
        .unwrap();
    kernel
        .create_room(&RoomConfig {
            name: "r2".into(),
            gravity: 1.0,
            capacity: 10,
        })
        .unwrap();
    kernel
        .store_tile(&Tile {
            id: "a".into(),
            room: "r1".into(),
            content: "data".into(),
            gravity: 1.0,
            created_at: 0,
        })
        .unwrap();
    kernel
        .store_tile(&Tile {
            id: "b".into(),
            room: "r2".into(),
            content: "other".into(),
            gravity: 1.0,
            created_at: 0,
        })
        .unwrap();
    assert_eq!(kernel.query_tiles("r1").unwrap().len(), 1);
    assert_eq!(kernel.query_tiles("r2").unwrap().len(), 1);
    assert_eq!(kernel.tile_count().unwrap(), 2);
}

#[test]
fn shell_sqlite_roundtrip() {
    let dir = TempDir::new().unwrap();
    let mut kernel = ShellKernel::bootstrap(dir.path()).unwrap();
    kernel
        .create_room(&RoomConfig {
            name: "vault".into(),
            gravity: 9.8,
            capacity: 999,
        })
        .unwrap();
    kernel
        .store_tile(&Tile {
            id: Uuid::new_v4().to_string(),
            room: "vault".into(),
            content: "heavy matter".into(),
            gravity: 9.8,
            created_at: 42,
        })
        .unwrap();

    // Re-open the database — data persists
    let kernel2 = ShellKernel::bootstrap(dir.path()).unwrap();
    let tiles = kernel2.query_tiles("vault").unwrap();
    assert_eq!(tiles.len(), 1);
    assert_eq!(tiles[0].content, "heavy matter");
    assert!((tiles[0].gravity - 9.8).abs() < f64::EPSILON);
}

#[test]
fn shell_tile_count() {
    let dir = TempDir::new().unwrap();
    let mut kernel = ShellKernel::bootstrap(dir.path()).unwrap();
    kernel
        .create_room(&RoomConfig {
            name: "count".into(),
            gravity: 1.0,
            capacity: 10,
        })
        .unwrap();
    for i in 0..5 {
        kernel
            .store_tile(&Tile {
                id: format!("t-{i}"),
                room: "count".into(),
                content: format!("item {i}"),
                gravity: 1.0,
                created_at: i,
            })
            .unwrap();
    }
    assert_eq!(kernel.tile_count().unwrap(), 5);
}

// ═══════════════════════════════════════════════════════════════════════════════
// 2. Provider + conservation
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn provider_single_completion() {
    let provider = MockProvider::new(100.0);
    let resp = provider.complete(&CompletionRequest {
        prompt: "hello world".into(),
        max_tokens: 50,
    });
    assert!(resp.tokens_used > 0);
    assert!(resp.cost > 0.0);
    assert!(resp.text.starts_with("mock:"));
}

#[test]
fn provider_tracks_budget() {
    let provider = MockProvider::new(10.0);
    provider.complete(&CompletionRequest {
        prompt: "test".into(),
        max_tokens: 50,
    });
    provider.complete(&CompletionRequest {
        prompt: "test2".into(),
        max_tokens: 50,
    });
    let ledger = provider.ledger();
    assert_eq!(ledger.completions, 2);
    assert!(ledger.spent > 0.0);
}

#[test]
fn conservation_law() {
    let provider = MockProvider::new(100.0);
    for _ in 0..10 {
        provider.complete(&CompletionRequest {
            prompt: "x".into(),
            max_tokens: 30,
        });
    }
    assert!(provider.ledger().verify_conservation());
}

#[test]
fn budget_remaining_decreases() {
    let provider = MockProvider::new(1.0);
    let before = provider.ledger().remaining();
    provider.complete(&CompletionRequest {
        prompt: "y".into(),
        max_tokens: 100,
    });
    let after = provider.ledger().remaining();
    assert!(after < before);
}

#[test]
fn multiple_providers_shared_ledger() {
    let budget = 50.0;
    let mut ledger = BudgetLedger::new(budget);
    let p1 = MockProvider::new(budget);
    let p2 = MockProvider::new(budget);
    let r1 = p1.complete(&CompletionRequest {
        prompt: "a".into(),
        max_tokens: 20,
    });
    let r2 = p2.complete(&CompletionRequest {
        prompt: "b".into(),
        max_tokens: 30,
    });
    ledger.record(r1.cost);
    ledger.record(r2.cost);
    assert!(ledger.verify_conservation());
    assert_eq!(ledger.completions, 2);
}

#[test]
fn provider_over_budget() {
    let provider = MockProvider::new(0.0);
    let resp = provider.complete(&CompletionRequest {
        prompt: "free".into(),
        max_tokens: 10,
    });
    assert!(resp.cost > 0.0);
    let ledger = provider.ledger();
    assert!(ledger.spent > 0.0);
    assert!(ledger.remaining() < 0.0);
}

// ═══════════════════════════════════════════════════════════════════════════════
// 3. Port + message routing
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn port_inject_and_drain() {
    let mut port = MemoryPort::new();
    port.inject(Message {
        id: "1".into(),
        source: "user".into(),
        target_room: "bridge".into(),
        body: "hello".into(),
        gravity: 1.0,
        timestamp: 0,
    });
    let msgs = port.drain();
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].body, "hello");
}

#[test]
fn port_drain_empties() {
    let mut port = MemoryPort::new();
    port.inject(Message {
        id: "1".into(),
        source: "a".into(),
        target_room: "r".into(),
        body: "x".into(),
        gravity: 1.0,
        timestamp: 0,
    });
    assert!(!port.drain().is_empty());
    assert!(port.drain().is_empty());
}

#[test]
fn route_by_gravity_high() {
    let port = MemoryPort::new();
    let rooms = vec![
        RoomConfig { name: "light".into(), gravity: 0.1, capacity: 10 },
        RoomConfig { name: "heavy".into(), gravity: 10.0, capacity: 10 },
    ];
    let msg = Message {
        id: "1".into(),
        source: "s".into(),
        target_room: String::new(),
        body: "x".into(),
        gravity: 9.5,
        timestamp: 0,
    };
    assert_eq!(port.route_by_gravity(&msg, &rooms).unwrap(), "heavy");
}

#[test]
fn route_by_gravity_low() {
    let port = MemoryPort::new();
    let rooms = vec![
        RoomConfig { name: "light".into(), gravity: 0.1, capacity: 10 },
        RoomConfig { name: "heavy".into(), gravity: 10.0, capacity: 10 },
    ];
    let msg = Message {
        id: "1".into(),
        source: "s".into(),
        target_room: String::new(),
        body: "x".into(),
        gravity: 0.2,
        timestamp: 0,
    };
    assert_eq!(port.route_by_gravity(&msg, &rooms).unwrap(), "light");
}

#[test]
fn port_multi_message_routing() {
    let port = MemoryPort::new();
    let rooms = vec![
        RoomConfig { name: "a".into(), gravity: 1.0, capacity: 10 },
        RoomConfig { name: "b".into(), gravity: 5.0, capacity: 10 },
        RoomConfig { name: "c".into(), gravity: 10.0, capacity: 10 },
    ];
    let msg_a = Message {
        id: "1".into(), source: "s".into(), target_room: String::new(),
        body: "lo".into(), gravity: 1.0, timestamp: 0,
    };
    let msg_b = Message {
        id: "2".into(), source: "s".into(), target_room: String::new(),
        body: "mid".into(), gravity: 5.0, timestamp: 0,
    };
    let msg_c = Message {
        id: "3".into(), source: "s".into(), target_room: String::new(),
        body: "hi".into(), gravity: 10.0, timestamp: 0,
    };
    assert_eq!(port.route_by_gravity(&msg_a, &rooms).unwrap(), "a");
    assert_eq!(port.route_by_gravity(&msg_b, &rooms).unwrap(), "b");
    assert_eq!(port.route_by_gravity(&msg_c, &rooms).unwrap(), "c");
}

#[test]
fn port_routes_map() {
    let mut port = MemoryPort::new();
    port.add_route("sensor-alpha", "lab");
    port.add_route("sensor-beta", "bridge");
    assert_eq!(port.routes["sensor-alpha"], "lab");
    assert_eq!(port.routes["sensor-beta"], "bridge");
}

// ═══════════════════════════════════════════════════════════════════════════════
// 4. Ensign + onboarding
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn ensign_creation() {
    let ensign = Ensign::new("e-001");
    assert_eq!(ensign.id, "e-001");
    assert_eq!(ensign.phase, 0);
    assert!(!ensign.ready);
}

#[test]
fn ensign_sequential_phases() {
    let mut ensign = Ensign::new("e-002");
    ensign.run_phase(1, "id").unwrap();
    ensign.run_phase(2, "caps").unwrap();
    ensign.run_phase(3, "scan").unwrap();
    assert_eq!(ensign.phase, 3);
}

#[test]
fn ensign_wrong_phase_fails() {
    let mut ensign = Ensign::new("e-003");
    assert!(ensign.run_phase(3, "skip").is_err());
}

#[test]
fn ensign_six_phase_onboarding() {
    let mut ensign = Ensign::new("e-004");
    let baton = ensign.full_onboarding(&["lab"]);
    assert_eq!(baton.phase, 6);
    assert!(baton.ready);
    assert!(baton.room_assignments.contains(&"lab".to_string()));
}

#[test]
fn ensign_baton_serialization() {
    let mut ensign = Ensign::new("e-005");
    let baton = ensign.full_onboarding(&["bridge", "lab"]);
    let json = serde_json::to_string(&baton).unwrap();
    let decoded: EnsignBaton = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded.ensign_id, "e-005");
    assert!(decoded.ready);
}

#[test]
fn ensign_multiple_room_assignments() {
    let mut ensign = Ensign::new("e-006");
    ensign.run_phase(1, "id").unwrap();
    ensign.run_phase(2, "caps").unwrap();
    ensign.run_phase(3, "scan").unwrap();
    ensign.run_phase(4, "bridge").unwrap();
    ensign.run_phase(5, "health").unwrap();
    ensign.run_phase(6, "ready").unwrap();
    // Room assigned during phase 4
    assert_eq!(ensign.room_assignments.len(), 1);
    assert!(ensign.room_assignments.contains(&"bridge".to_string()));
}

#[test]
fn ensign_not_ready_before_complete() {
    let mut ensign = Ensign::new("e-007");
    ensign.run_phase(1, "id").unwrap();
    assert!(!ensign.ready);
    assert!(!ensign.baton().ready);
}

// ═══════════════════════════════════════════════════════════════════════════════
// 5. Circuit + deadband
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn circuit_starts_normal() {
    let circuit = MonitoringCircuit::new(0.0, 20.0, 80.0, 100.0, 5.0);
    assert_eq!(circuit.status, CircuitStatus::Normal);
}

#[test]
fn circuit_tick_normal_value() {
    let mut circuit = MonitoringCircuit::new(0.0, 20.0, 80.0, 100.0, 5.0);
    let status = circuit.tick(50.0);
    assert_eq!(status, CircuitStatus::Normal);
}

#[test]
fn circuit_tick_warning_high() {
    let mut circuit = MonitoringCircuit::new(0.0, 20.0, 80.0, 100.0, 5.0);
    let status = circuit.tick(85.0);
    assert_eq!(status, CircuitStatus::Warning);
}

#[test]
fn circuit_tick_critical_high() {
    let mut circuit = MonitoringCircuit::new(0.0, 20.0, 80.0, 100.0, 5.0);
    let status = circuit.tick(100.0);
    assert_eq!(status, CircuitStatus::Critical);
}

#[test]
fn circuit_tick_warning_low() {
    let mut circuit = MonitoringCircuit::new(0.0, 20.0, 80.0, 100.0, 5.0);
    let status = circuit.tick(15.0);
    assert_eq!(status, CircuitStatus::Warning);
}

#[test]
fn circuit_tick_critical_low() {
    let mut circuit = MonitoringCircuit::new(0.0, 20.0, 80.0, 100.0, 5.0);
    let status = circuit.tick(0.0);
    assert_eq!(status, CircuitStatus::Critical);
}

#[test]
fn circuit_tick_count_increments() {
    let mut circuit = MonitoringCircuit::new(0.0, 20.0, 80.0, 100.0, 5.0);
    for _ in 0..10 {
        circuit.tick(50.0);
    }
    assert_eq!(circuit.tick_count(), 10);
}

#[test]
fn circuit_status_change_detection() {
    let mut circuit = MonitoringCircuit::new(0.0, 20.0, 80.0, 100.0, 5.0);
    // First tick stays normal — no change from initial Normal
    let (_, changed) = circuit.tick_with_status(50.0);
    assert!(!changed); // Normal -> Normal
    // Now go to warning
    let (status, changed) = circuit.tick_with_status(85.0);
    assert_eq!(status, CircuitStatus::Warning);
    assert!(changed);
}

// ═══════════════════════════════════════════════════════════════════════════════
// 6. Penrose + correlations
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn penrose_no_correlation_insufficient_data() {
    let mut det = PenroseDetector::new(0.8);
    det.feed(&Signal { room: "a".into(), value: 1.0, timestamp: 0 });
    det.feed(&Signal { room: "b".into(), value: 1.0, timestamp: 0 });
    assert!(det.detect().is_empty());
}

#[test]
fn penrose_perfect_correlation() {
    let mut det = PenroseDetector::new(0.8);
    for i in 0..10 {
        let v = i as f64;
        det.feed(&Signal { room: "a".into(), value: v, timestamp: i });
        det.feed(&Signal { room: "b".into(), value: v * 2.0, timestamp: i });
    }
    let corrs = det.detect();
    assert_eq!(corrs.len(), 1);
    assert!((corrs[0].coefficient - 1.0).abs() < 0.01);
}

#[test]
fn penrose_no_correlation_random() {
    let mut det = PenroseDetector::new(0.9);
    for i in 0..10 {
        det.feed(&Signal { room: "x".into(), value: i as f64, timestamp: i });
        det.feed(&Signal { room: "y".into(), value: (10 - i) as f64, timestamp: i });
    }
    // Negative correlation should not pass 0.9 threshold for absolute value
    // Actually -1.0 has abs 1.0 > 0.9, so this WILL detect.
    // Let's use a threshold that excludes it
    let mut det2 = PenroseDetector::new(0.99);
    for i in 0..10 {
        det2.feed(&Signal { room: "x".into(), value: i as f64, timestamp: i });
        det2.feed(&Signal { room: "y".into(), value: (i * 3 % 7) as f64, timestamp: i });
    }
    // Random-ish data should have |correlation| < 0.99
    assert!(det2.detect().is_empty());
}

#[test]
fn penrose_spline_id_created() {
    let mut det = PenroseDetector::new(0.5);
    for i in 0..5 {
        det.feed(&Signal { room: "a".into(), value: i as f64, timestamp: i });
        det.feed(&Signal { room: "b".into(), value: i as f64 * 3.0, timestamp: i });
    }
    let corrs = det.detect();
    assert!(!corrs[0].spline_id.is_empty());
}

#[test]
fn penrose_accumulates_correlations() {
    let mut det = PenroseDetector::new(0.5);
    for i in 0..5 {
        det.feed(&Signal { room: "a".into(), value: i as f64, timestamp: i });
        det.feed(&Signal { room: "b".into(), value: i as f64, timestamp: i });
    }
    det.detect();
    assert_eq!(det.correlations().len(), 1);
    det.detect(); // re-detect, should accumulate
    assert_eq!(det.correlations().len(), 2);
}

// ═══════════════════════════════════════════════════════════════════════════════
// 7. Git agent + provenance
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn git_agent_init() {
    let dir = TempDir::new().unwrap();
    let agent = GitAgent::init(dir.path()).unwrap();
    assert!(agent.ledger().is_empty());
}

#[test]
fn git_agent_record_entry() {
    let dir = TempDir::new().unwrap();
    let mut agent = GitAgent::init(dir.path()).unwrap();
    agent.record(ProvenanceEntry {
        id: "p-1".into(),
        action: "create_tile".into(),
        agent: "ensign-1".into(),
        room: "lab".into(),
        timestamp: 100,
        metadata: HashMap::new(),
    });
    assert_eq!(agent.ledger().len(), 1);
    assert_eq!(agent.ledger()[0].action, "create_tile");
}

#[test]
fn git_agent_disk_roundtrip() {
    let dir = TempDir::new().unwrap();
    let mut agent = GitAgent::init(dir.path()).unwrap();
    for i in 0..3 {
        agent.record(ProvenanceEntry {
            id: format!("p-{i}"),
            action: "action".into(),
            agent: "bot".into(),
            room: "room".into(),
            timestamp: i,
            metadata: HashMap::new(),
        });
    }
    let from_disk = agent.read_ledger_from_disk();
    assert_eq!(from_disk.len(), 3);
}

#[test]
fn git_agent_metadata_preserved() {
    let dir = TempDir::new().unwrap();
    let mut agent = GitAgent::init(dir.path()).unwrap();
    let mut meta = HashMap::new();
    meta.insert("key".into(), "value".into());
    agent.record(ProvenanceEntry {
        id: "p-meta".into(),
        action: "test".into(),
        agent: "bot".into(),
        room: "room".into(),
        timestamp: 0,
        metadata: meta,
    });
    let from_disk = agent.read_ledger_from_disk();
    assert_eq!(from_disk[0].metadata["key"], "value");
}

#[test]
fn git_agent_empty_disk_read() {
    let dir = TempDir::new().unwrap();
    let agent = GitAgent::init(dir.path()).unwrap();
    assert!(agent.read_ledger_from_disk().is_empty());
}

// ═══════════════════════════════════════════════════════════════════════════════
// 8. Shell spawn + sandboxing
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn zero_claw_spawn() {
    let dir = TempDir::new().unwrap();
    let universe = Universe::create(dir.path()).unwrap();
    let claw = ZeroClaw::spawn("zc-1", universe);
    assert_eq!(claw.id(), "zc-1");
}

#[test]
fn zero_claw_can_access_own_universe() {
    let dir = TempDir::new().unwrap();
    let universe = Universe::create(dir.path()).unwrap();
    let claw = ZeroClaw::spawn("zc-2", universe);
    let inner = dir.path().join("inner.txt");
    assert!(claw.try_access(&inner).is_ok());
}

#[test]
fn zero_claw_blocked_outside_universe() {
    let dir = TempDir::new().unwrap();
    let universe = Universe::create(dir.path()).unwrap();
    let claw = ZeroClaw::spawn("zc-3", universe);
    let outside = PathBuf::from("/etc/passwd");
    assert!(claw.try_access(&outside).is_err());
}

#[test]
fn universe_is_within_check() {
    let dir = TempDir::new().unwrap();
    let universe = Universe::create(dir.path()).unwrap();
    let child = dir.path().join("subdir").join("file.txt");
    assert!(universe.is_within(&child));
    assert!(!universe.is_within(Path::new("/tmp/other")));
}

#[test]
fn zero_claw_universe_root() {
    let dir = TempDir::new().unwrap();
    let universe = Universe::create(dir.path()).unwrap();
    let claw = ZeroClaw::spawn("zc-4", universe);
    assert!(claw.universe_root().starts_with(dir.path()));
}

// ═══════════════════════════════════════════════════════════════════════════════
// 9. Async tick + priority
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn tick_queue_ordering() {
    let mut queue = TickQueue::new();
    queue.enqueue(TickTask {
        id: "1".into(),
        priority: Priority::Low,
        payload: "low".into(),
        executed: false,
    });
    queue.enqueue(TickTask {
        id: "2".into(),
        priority: Priority::Critical,
        payload: "critical".into(),
        executed: false,
    });
    queue.enqueue(TickTask {
        id: "3".into(),
        priority: Priority::Normal,
        payload: "normal".into(),
        executed: false,
    });
    let results = queue.drain_by_priority();
    assert_eq!(results[0], "critical");
    assert_eq!(results[1], "normal");
    assert_eq!(results[2], "low");
}

#[test]
fn tick_queue_dedup() {
    let mut queue = TickQueue::new();
    queue.enqueue(TickTask {
        id: "1".into(),
        priority: Priority::High,
        payload: "do-once".into(),
        executed: false,
    });
    queue.drain_by_priority();
    let results = queue.drain_by_priority();
    assert!(results.is_empty()); // all already executed
}

#[test]
fn tick_queue_execution_log() {
    let mut queue = TickQueue::new();
    queue.enqueue(TickTask {
        id: "1".into(),
        priority: Priority::High,
        payload: "a".into(),
        executed: false,
    });
    queue.enqueue(TickTask {
        id: "2".into(),
        priority: Priority::Low,
        payload: "b".into(),
        executed: false,
    });
    queue.drain_by_priority();
    assert_eq!(queue.execution_log().len(), 2);
    assert!(queue.execution_log()[0].contains("High"));
}

#[test]
fn tick_priority_ordering() {
    assert!(Priority::Critical > Priority::High);
    assert!(Priority::High > Priority::Normal);
    assert!(Priority::Normal > Priority::Low);
}

// ═══════════════════════════════════════════════════════════════════════════════
// 10. Full stack integration
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn full_stack_bootstrap_to_response() {
    let dir = TempDir::new().unwrap();

    // 1. Bootstrap shell
    let mut kernel = ShellKernel::bootstrap(dir.path()).unwrap();

    // 2. Add rooms
    let bridge = RoomConfig { name: "bridge".into(), gravity: 1.0, capacity: 10 };
    let lab = RoomConfig { name: "lab".into(), gravity: 0.5, capacity: 10 };
    kernel.create_room(&bridge).unwrap();
    kernel.create_room(&lab).unwrap();

    // 3. Deploy ensign
    let mut ensign = Ensign::new("ensign-fullstack");
    let baton = ensign.full_onboarding(&["bridge"]);
    assert!(baton.ready);

    // 4. Receive message
    let mut port = MemoryPort::new();
    port.inject(Message {
        id: "msg-1".into(),
        source: "user".into(),
        target_room: String::new(),
        body: "status report".into(),
        gravity: 1.0,
        timestamp: 100,
    });

    // 5. Route message
    let rooms = vec![bridge.clone(), lab.clone()];
    let msgs = port.drain();
    let target = port.route_by_gravity(&msgs[0], &rooms).unwrap();
    assert_eq!(target, "bridge");

    // 6. Respond via provider
    let provider = MockProvider::new(100.0);
    let response = provider.complete(&CompletionRequest {
        prompt: "status report".into(),
        max_tokens: 50,
    });
    assert!(!response.text.is_empty());

    // 7. Record provenance
    let mut git_agent = GitAgent::init(&dir.path().join("provenance")).unwrap();
    git_agent.record(ProvenanceEntry {
        id: "prov-1".into(),
        action: "respond".into(),
        agent: "ensign-fullstack".into(),
        room: "bridge".into(),
        timestamp: 100,
        metadata: HashMap::new(),
    });
    assert_eq!(git_agent.ledger().len(), 1);

    // 8. Verify conservation
    assert!(provider.ledger().verify_conservation());

    // 9. Store result tile
    kernel
        .store_tile(&Tile {
            id: "tile-response".into(),
            room: "bridge".into(),
            content: response.text.clone(),
            gravity: 1.0,
            created_at: 100,
        })
        .unwrap();
    let tiles = kernel.query_tiles("bridge").unwrap();
    assert_eq!(tiles.len(), 1);
}

#[test]
fn full_stack_with_circuit_monitoring() {
    let dir = TempDir::new().unwrap();
    let mut kernel = ShellKernel::bootstrap(dir.path()).unwrap();
    kernel
        .create_room(&RoomConfig {
            name: "reactor".into(),
            gravity: 5.0,
            capacity: 10,
        })
        .unwrap();

    // Monitor circuit
    let mut circuit = MonitoringCircuit::new(0.0, 20.0, 80.0, 100.0, 5.0);
    let readings = [50.0, 75.0, 85.0, 50.0];
    for &val in &readings {
        circuit.tick(val);
    }
    assert_eq!(circuit.tick_count(), 4);

    // Store circuit readings as tiles
    for (i, &val) in readings.iter().enumerate() {
        kernel
            .store_tile(&Tile {
                id: format!("reading-{i}"),
                room: "reactor".into(),
                content: format!("temp={val}"),
                gravity: 5.0,
                created_at: i as i64,
            })
            .unwrap();
    }
    assert_eq!(kernel.tile_count().unwrap(), 4);
}

#[test]
fn full_stack_correlation_detection() {
    let mut det = PenroseDetector::new(0.7);
    // Two correlated rooms
    for i in 0..10 {
        let v = i as f64 * 2.0;
        det.feed(&Signal { room: "sensor-a".into(), value: v, timestamp: i });
        det.feed(&Signal { room: "sensor-b".into(), value: v + 1.0, timestamp: i });
    }
    let corrs = det.detect();
    assert_eq!(corrs.len(), 1);
    assert!(corrs[0].coefficient > 0.7);
}

#[test]
fn full_stack_priority_task_processing() {
    let mut queue = TickQueue::new();
    // Simulate a full stack with priority-based task ordering
    queue.enqueue(TickTask {
        id: "log".into(),
        priority: Priority::Low,
        payload: "write-log".into(),
        executed: false,
    });
    queue.enqueue(TickTask {
        id: "alert".into(),
        priority: Priority::Critical,
        payload: "send-alert".into(),
        executed: false,
    });
    queue.enqueue(TickTask {
        id: "process".into(),
        priority: Priority::Normal,
        payload: "process-msg".into(),
        executed: false,
    });
    queue.enqueue(TickTask {
        id: "respond".into(),
        priority: Priority::High,
        payload: "gen-response".into(),
        executed: false,
    });

    let order = queue.drain_by_priority();
    assert_eq!(order, vec!["send-alert", "gen-response", "process-msg", "write-log"]);
}

#[test]
fn full_stack_sandbox_isolation() {
    let dir = TempDir::new().unwrap();
    let universe = Universe::create(dir.path()).unwrap();
    let claw = ZeroClaw::spawn("sandbox-test", universe);

    // Can access own space
    assert!(claw.try_access(&dir.path().join("work")).is_ok());
    // Cannot escape
    assert!(claw.try_access(Path::new("/etc/shadow")).is_err());
    assert!(claw.try_access(Path::new("/root")).is_err());
}

#[test]
fn full_stack_provenance_chain() {
    let dir = TempDir::new().unwrap();
    let mut agent = GitAgent::init(&dir.path().join("prov")).unwrap();

    // Simulate a chain of actions
    let actions = ["bootstrap", "create_room", "deploy_ensign", "receive_msg", "respond"];
    for (i, action) in actions.iter().enumerate() {
        agent.record(ProvenanceEntry {
            id: format!("chain-{i}"),
            action: (*action).into(),
            agent: "ensign-1".into(),
            room: "bridge".into(),
            timestamp: i as i64 * 100,
            metadata: HashMap::new(),
        });
    }
    assert_eq!(agent.ledger().len(), 5);
    let from_disk = agent.read_ledger_from_disk();
    assert_eq!(from_disk.len(), 5);

    // Verify ordering
    for (i, entry) in from_disk.iter().enumerate() {
        assert_eq!(entry.action, actions[i]);
    }
}

#[test]
fn full_stack_budget_conservation_under_load() {
    let provider = MockProvider::new(50.0);
    let mut total_cost = 0.0;
    for i in 0..20 {
        let resp = provider.complete(&CompletionRequest {
            prompt: format!("query-{i}"),
            max_tokens: 50,
        });
        total_cost += resp.cost;
    }
    let ledger = provider.ledger();
    assert!((ledger.spent - total_cost).abs() < f64::EPSILON);
    assert!(ledger.verify_conservation());
    assert_eq!(ledger.completions, 20);
}
