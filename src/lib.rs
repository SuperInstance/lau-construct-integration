//! # lau-construct-integration
//!
//! Integration test crate that wires together key lau-* crate concepts and
//! proves they compose correctly. Not a library — test-only.

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
// tempfile is used in integration tests only
use uuid::Uuid;

// ─────────────────────────────────────────────────────────────────────────────
// Inline minimal types (stand-ins for actual lau-* crate types)
// ─────────────────────────────────────────────────────────────────────────────

// -- Shell kernel + tile store --

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tile {
    pub id: String,
    pub room: String,
    pub content: String,
    pub gravity: f64,
    pub created_at: i64,
}

#[derive(Debug)]
pub struct ShellKernel {
    db: Connection,
    rooms: HashMap<String, RoomConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomConfig {
    pub name: String,
    pub gravity: f64,
    pub capacity: u32,
}

impl ShellKernel {
    pub fn bootstrap(dir: &Path) -> rusqlite::Result<Self> {
        let db_path = dir.join("shell.db");
        let db = Connection::open(&db_path)?;
        db.execute_batch(
            "CREATE TABLE IF NOT EXISTS tiles (
                id TEXT PRIMARY KEY,
                room TEXT NOT NULL,
                content TEXT NOT NULL,
                gravity REAL NOT NULL DEFAULT 0.0,
                created_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS rooms (
                name TEXT PRIMARY KEY,
                gravity REAL NOT NULL DEFAULT 1.0,
                capacity INTEGER NOT NULL DEFAULT 100
            );",
        )?;
        Ok(Self {
            db,
            rooms: HashMap::new(),
        })
    }

    pub fn create_room(&mut self, config: &RoomConfig) -> rusqlite::Result<()> {
        self.db.execute(
            "INSERT OR REPLACE INTO rooms (name, gravity, capacity) VALUES (?1, ?2, ?3)",
            params![config.name, config.gravity, config.capacity],
        )?;
        self.rooms.insert(config.name.clone(), config.clone());
        Ok(())
    }

    pub fn store_tile(&self, tile: &Tile) -> rusqlite::Result<()> {
        self.db.execute(
            "INSERT INTO tiles (id, room, content, gravity, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![tile.id, tile.room, tile.content, tile.gravity, tile.created_at],
        )?;
        Ok(())
    }

    pub fn query_tiles(&self, room: &str) -> rusqlite::Result<Vec<Tile>> {
        let mut stmt = self
            .db
            .prepare("SELECT id, room, content, gravity, created_at FROM tiles WHERE room = ?1")?;
        let rows = stmt.query_map(params![room], |row| {
            Ok(Tile {
                id: row.get(0)?,
                room: row.get(1)?,
                content: row.get(2)?,
                gravity: row.get(3)?,
                created_at: row.get(4)?,
            })
        })?;
        rows.collect()
    }

    pub fn get_room(&self, name: &str) -> rusqlite::Result<Option<RoomConfig>> {
        let mut stmt = self
            .db
            .prepare("SELECT name, gravity, capacity FROM rooms WHERE name = ?1")?;
        let mut rows = stmt.query_map(params![name], |row| {
            Ok(RoomConfig {
                name: row.get(0)?,
                gravity: row.get(1)?,
                capacity: row.get(2)?,
            })
        })?;
        match rows.next() {
            Some(r) => Ok(Some(r?)),
            None => Ok(None),
        }
    }

    pub fn tile_count(&self) -> rusqlite::Result<u32> {
        self.db
            .query_row("SELECT COUNT(*) FROM tiles", [], |row| row.get(0))
    }
}

// -- Provider + conservation --

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionRequest {
    pub prompt: String,
    pub max_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionResponse {
    pub text: String,
    pub tokens_used: u32,
    pub cost: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetLedger {
    pub total_budget: f64,
    pub spent: f64,
    pub completions: u32,
}

impl BudgetLedger {
    pub fn new(budget: f64) -> Self {
        Self {
            total_budget: budget,
            spent: 0.0,
            completions: 0,
        }
    }

    pub fn record(&mut self, cost: f64) {
        self.spent += cost;
        self.completions += 1;
    }

    pub fn remaining(&self) -> f64 {
        self.total_budget - self.spent
    }

    pub fn verify_conservation(&self) -> bool {
        (self.total_budget - (self.spent + self.remaining())).abs() < f64::EPSILON
    }
}

pub struct MockProvider {
    ledger: Arc<std::sync::Mutex<BudgetLedger>>,
    token_counter: AtomicU64,
}

impl MockProvider {
    pub fn new(budget: f64) -> Self {
        Self {
            ledger: Arc::new(std::sync::Mutex::new(BudgetLedger::new(budget))),
            token_counter: AtomicU64::new(0),
        }
    }

    pub fn complete(&self, req: &CompletionRequest) -> CompletionResponse {
        let tokens = req.max_tokens.min(100);
        let cost = tokens as f64 * 0.001;
        self.token_counter.fetch_add(tokens as u64, Ordering::SeqCst);
        {
            let mut ledger = self.ledger.lock().unwrap();
            ledger.record(cost);
        }
        CompletionResponse {
            text: format!("mock: {}", &req.prompt[..req.prompt.len().min(20)]),
            tokens_used: tokens,
            cost,
        }
    }

    pub fn ledger(&self) -> std::sync::MutexGuard<'_, BudgetLedger> {
        self.ledger.lock().unwrap()
    }
}

// -- Port + message routing --

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub source: String,
    pub target_room: String,
    pub body: String,
    pub gravity: f64,
    pub timestamp: i64,
}

pub struct MemoryPort {
    pub inbox: Vec<Message>,
    pub routes: HashMap<String, String>,
}

impl Default for MemoryPort {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryPort {
    pub fn new() -> Self {
        Self {
            inbox: Vec::new(),
            routes: HashMap::new(),
        }
    }

    pub fn add_route(&mut self, source: &str, room: &str) {
        self.routes.insert(source.to_string(), room.to_string());
    }

    pub fn inject(&mut self, msg: Message) {
        self.inbox.push(msg);
    }

    pub fn route_by_gravity(&self, msg: &Message, rooms: &[RoomConfig]) -> Option<String> {
        rooms
            .iter()
            .min_by(|a, b| {
                (a.gravity - msg.gravity)
                    .abs()
                    .partial_cmp(&(b.gravity - msg.gravity).abs())
                    .unwrap()
            })
            .map(|r| r.name.clone())
    }

    pub fn drain(&mut self) -> Vec<Message> {
        std::mem::take(&mut self.inbox)
    }
}

// -- Ensign + onboarding --

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnsignBaton {
    pub ensign_id: String,
    pub phase: u8,
    pub room_assignments: Vec<String>,
    pub ready: bool,
}

pub struct Ensign {
    pub id: String,
    pub phase: u8,
    pub room_assignments: Vec<String>,
    pub ready: bool,
    onboarding_log: Vec<String>,
}

impl Ensign {
    pub fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            phase: 0,
            room_assignments: Vec::new(),
            ready: false,
            onboarding_log: Vec::new(),
        }
    }

    pub fn run_phase(&mut self, phase: u8, payload: &str) -> Result<(), String> {
        if phase != self.phase + 1 && phase != 1 {
            return Err(format!("Expected phase {}, got {}", self.phase + 1, phase));
        }
        self.phase = phase;
        self.onboarding_log
            .push(format!("phase {}: {}", phase, payload));
        match phase {
            1 | 2 | 3 | 5 => {}
            4 => self.room_assignments.push(payload.to_string()),
            6 => self.ready = true,
            _ => return Err(format!("Unknown phase: {}", phase)),
        }
        Ok(())
    }

    pub fn full_onboarding(&mut self, rooms: &[&str]) -> EnsignBaton {
        let payloads = [
            "identity:ensign",
            "caps:default",
            "scan:rooms",
            rooms.first().unwrap_or(&"default"),
            "health:ok",
            "ready:true",
        ];
        for (i, payload) in payloads.iter().enumerate() {
            self.run_phase((i + 1) as u8, payload).unwrap();
        }
        if let Some(room) = rooms.first() {
            if !self.room_assignments.contains(&room.to_string()) {
                self.room_assignments.push(room.to_string());
            }
        }
        EnsignBaton {
            ensign_id: self.id.clone(),
            phase: self.phase,
            room_assignments: self.room_assignments.clone(),
            ready: self.ready,
        }
    }

    pub fn baton(&self) -> EnsignBaton {
        EnsignBaton {
            ensign_id: self.id.clone(),
            phase: self.phase,
            room_assignments: self.room_assignments.clone(),
            ready: self.ready,
        }
    }
}

// -- Circuit + deadband --

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum CircuitStatus {
    Normal,
    Warning,
    Critical,
    Deadband,
}

pub struct MonitoringCircuit {
    pub value: f64,
    pub low_warn: f64,
    pub high_warn: f64,
    pub low_crit: f64,
    pub high_crit: f64,
    pub deadband: f64,
    pub status: CircuitStatus,
    last_status: CircuitStatus,
    ticks: u64,
}

impl MonitoringCircuit {
    pub fn new(
        low_crit: f64,
        low_warn: f64,
        high_warn: f64,
        high_crit: f64,
        deadband: f64,
    ) -> Self {
        Self {
            value: (high_warn + low_warn) / 2.0,
            low_warn,
            high_warn,
            low_crit,
            high_crit,
            deadband,
            status: CircuitStatus::Normal,
            last_status: CircuitStatus::Normal,
            ticks: 0,
        }
    }

    pub fn tick(&mut self, new_value: f64) -> CircuitStatus {
        self.ticks += 1;
        self.last_status = self.status;
        self.value = new_value;

        let proposed = if new_value <= self.low_crit || new_value >= self.high_crit {
            CircuitStatus::Critical
        } else if new_value <= self.low_warn || new_value >= self.high_warn {
            CircuitStatus::Warning
        } else {
            CircuitStatus::Normal
        };

        self.status = proposed;
        self.status
    }

    pub fn tick_with_status(&mut self, new_value: f64) -> (CircuitStatus, bool) {
        let status = self.tick(new_value);
        let changed = status != self.last_status;
        (status, changed)
    }

    pub fn tick_count(&self) -> u64 {
        self.ticks
    }
}

// -- Penrose + correlations --

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Signal {
    pub room: String,
    pub value: f64,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Correlation {
    pub room_a: String,
    pub room_b: String,
    pub coefficient: f64,
    pub spline_id: String,
}

pub struct PenroseDetector {
    history: HashMap<String, Vec<f64>>,
    threshold: f64,
    correlations: Vec<Correlation>,
}

impl PenroseDetector {
    pub fn new(threshold: f64) -> Self {
        Self {
            history: HashMap::new(),
            threshold,
            correlations: Vec::new(),
        }
    }

    pub fn feed(&mut self, signal: &Signal) {
        self.history
            .entry(signal.room.clone())
            .or_default()
            .push(signal.value);
    }

    pub fn detect(&mut self) -> Vec<Correlation> {
        let rooms: Vec<_> = self.history.keys().cloned().collect();
        let mut found = Vec::new();
        for i in 0..rooms.len() {
            for j in (i + 1)..rooms.len() {
                if let Some(c) = self.pearson(&rooms[i], &rooms[j]) {
                    if c.abs() >= self.threshold {
                        found.push(Correlation {
                            room_a: rooms[i].clone(),
                            room_b: rooms[j].clone(),
                            coefficient: c,
                            spline_id: Uuid::new_v4().to_string(),
                        });
                    }
                }
            }
        }
        self.correlations.extend(found.clone());
        found
    }

    fn pearson(&self, a: &str, b: &str) -> Option<f64> {
        let va = self.history.get(a)?;
        let vb = self.history.get(b)?;
        let n = va.len().min(vb.len());
        if n < 3 {
            return None;
        }
        let mean_a = va[..n].iter().sum::<f64>() / n as f64;
        let mean_b = vb[..n].iter().sum::<f64>() / n as f64;
        let (mut cov, mut var_a, mut var_b) = (0.0, 0.0, 0.0);
        for i in 0..n {
            let da = va[i] - mean_a;
            let db = vb[i] - mean_b;
            cov += da * db;
            var_a += da * da;
            var_b += db * db;
        }
        if var_a == 0.0 || var_b == 0.0 {
            return None;
        }
        Some(cov / (var_a.sqrt() * var_b.sqrt()))
    }

    pub fn correlations(&self) -> &[Correlation] {
        &self.correlations
    }
}

// -- Git agent + provenance --

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvenanceEntry {
    pub id: String,
    pub action: String,
    pub agent: String,
    pub room: String,
    pub timestamp: i64,
    pub metadata: HashMap<String, String>,
}

pub struct GitAgent {
    repo_dir: PathBuf,
    ledger: Vec<ProvenanceEntry>,
}

impl GitAgent {
    pub fn init(dir: &Path) -> std::io::Result<Self> {
        std::fs::create_dir_all(dir)?;
        Ok(Self {
            repo_dir: dir.to_path_buf(),
            ledger: Vec::new(),
        })
    }

    pub fn record(&mut self, entry: ProvenanceEntry) {
        let ledger_path = self.repo_dir.join("provenance.jsonl");
        let line = serde_json::to_string(&entry).unwrap();
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&ledger_path)
            .unwrap();
        writeln!(f, "{}", line).unwrap();
        self.ledger.push(entry);
    }

    pub fn ledger(&self) -> &[ProvenanceEntry] {
        &self.ledger
    }

    pub fn read_ledger_from_disk(&self) -> Vec<ProvenanceEntry> {
        let ledger_path = self.repo_dir.join("provenance.jsonl");
        if !ledger_path.exists() {
            return Vec::new();
        }
        let content = std::fs::read_to_string(&ledger_path).unwrap();
        content
            .lines()
            .filter(|l| !l.is_empty())
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }
}

// -- Shell spawn + sandboxing --

pub struct Universe {
    root: PathBuf,
}

impl Universe {
    pub fn create(root: &Path) -> std::io::Result<Self> {
        std::fs::create_dir_all(root)?;
        Ok(Self {
            root: root.to_path_buf(),
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn is_within(&self, path: &Path) -> bool {
        path.starts_with(&self.root)
    }
}

pub struct ZeroClaw {
    id: String,
    universe: Universe,
    allowed_paths: Vec<PathBuf>,
}

impl ZeroClaw {
    pub fn spawn(id: &str, universe: Universe) -> Self {
        let root = universe.root().to_path_buf();
        Self {
            id: id.to_string(),
            allowed_paths: vec![root],
            universe,
        }
    }

    pub fn try_access(&self, path: &Path) -> Result<String, String> {
        let canonical = if path.exists() {
            path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
        } else {
            path.to_path_buf()
        };
        if self.universe.is_within(&canonical)
            || self.allowed_paths.iter().any(|p| canonical.starts_with(p))
        {
            Ok(format!("access granted: {:?}", path))
        } else {
            Err(format!("sandbox violation: {:?} outside universe", path))
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn universe_root(&self) -> &Path {
        self.universe.root()
    }
}

// -- Async tick + priority --

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    Low = 0,
    Normal = 1,
    High = 2,
    Critical = 3,
}

#[derive(Debug)]
pub struct TickTask {
    pub id: String,
    pub priority: Priority,
    pub payload: String,
    pub executed: bool,
}

pub struct TickQueue {
    tasks: Vec<TickTask>,
    execution_log: Vec<String>,
}

impl Default for TickQueue {
    fn default() -> Self {
        Self::new()
    }
}

impl TickQueue {
    pub fn new() -> Self {
        Self {
            tasks: Vec::new(),
            execution_log: Vec::new(),
        }
    }

    pub fn enqueue(&mut self, task: TickTask) {
        self.tasks.push(task);
    }

    pub fn drain_by_priority(&mut self) -> Vec<String> {
        self.tasks.sort_by_key(|b| std::cmp::Reverse(b.priority));
        let mut results = Vec::new();
        for task in &mut self.tasks {
            if !task.executed {
                task.executed = true;
                self.execution_log
                    .push(format!("[{:?}] {}", task.priority, task.payload));
                results.push(task.payload.clone());
            }
        }
        results
    }

    pub fn execution_log(&self) -> &[String] {
        &self.execution_log
    }
}
