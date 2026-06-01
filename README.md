# lau-construct-integration

> Integration tests proving lau-* crates compose correctly

## What This Does

Integration tests proving lau-* crates compose correctly. Part of the PLATO/LAU ecosystem — a mathematically rigorous framework for building educational agents that learn, teach, and evolve.

## The Key Idea

This crate implements the core abstractions needed for its domain, with a focus on correctness, composability, and conservation guarantees. Every public type is serializable (serde), every algorithm is tested, and every invariant is verified.

## Install

```bash
cargo add lau-construct-integration
```

## Quick Start

See the API Reference below for complete usage. Key entry points:

```rust
use lau_construct_integration::*;
// See types and methods below for complete usage
```

## API Reference

```rust
pub struct Tile 
pub struct ShellKernel 
pub struct RoomConfig 
    pub fn bootstrap(dir: &Path) -> rusqlite::Result<Self> 
    pub fn create_room(&mut self, config: &RoomConfig) -> rusqlite::Result<()> 
    pub fn store_tile(&self, tile: &Tile) -> rusqlite::Result<()> 
    pub fn query_tiles(&self, room: &str) -> rusqlite::Result<Vec<Tile>> 
    pub fn get_room(&self, name: &str) -> rusqlite::Result<Option<RoomConfig>> 
    pub fn tile_count(&self) -> rusqlite::Result<u32> 
pub struct CompletionRequest 
pub struct CompletionResponse 
pub struct BudgetLedger 
    pub fn new(budget: f64) -> Self 
    pub fn record(&mut self, cost: f64) 
    pub fn remaining(&self) -> f64 
    pub fn verify_conservation(&self) -> bool 
pub struct MockProvider 
    pub fn new(budget: f64) -> Self 
    pub fn complete(&self, req: &CompletionRequest) -> CompletionResponse 
    pub fn ledger(&self) -> std::sync::MutexGuard<'_, BudgetLedger> 
pub struct Message 
pub struct MemoryPort 
    pub fn new() -> Self 
    pub fn add_route(&mut self, source: &str, room: &str) 
    pub fn inject(&mut self, msg: Message) 
    pub fn route_by_gravity(&self, msg: &Message, rooms: &[RoomConfig]) -> Option<String> 
    pub fn drain(&mut self) -> Vec<Message> 
pub struct EnsignBaton 
pub struct Ensign 
    pub fn new(id: &str) -> Self 
    pub fn run_phase(&mut self, phase: u8, payload: &str) -> Result<(), String> 
    pub fn full_onboarding(&mut self, rooms: &[&str]) -> EnsignBaton 
    pub fn baton(&self) -> EnsignBaton 
pub enum CircuitStatus 
pub struct MonitoringCircuit 
    pub fn new(
    pub fn tick(&mut self, new_value: f64) -> CircuitStatus 
    pub fn tick_with_status(&mut self, new_value: f64) -> (CircuitStatus, bool) 
    pub fn tick_count(&self) -> u64 
pub struct Signal 
pub struct Correlation 
pub struct PenroseDetector 
    pub fn new(threshold: f64) -> Self 
    pub fn feed(&mut self, signal: &Signal) 
    pub fn detect(&mut self) -> Vec<Correlation> 
    pub fn correlations(&self) -> &[Correlation] 
pub struct ProvenanceEntry 
pub struct GitAgent 
    pub fn init(dir: &Path) -> std::io::Result<Self> 
    pub fn record(&mut self, entry: ProvenanceEntry) 
    pub fn ledger(&self) -> &[ProvenanceEntry] 
    pub fn read_ledger_from_disk(&self) -> Vec<ProvenanceEntry> 
pub struct Universe 
    pub fn create(root: &Path) -> std::io::Result<Self> 
    pub fn root(&self) -> &Path 
    pub fn is_within(&self, path: &Path) -> bool 
pub struct ZeroClaw 
    pub fn spawn(id: &str, universe: Universe) -> Self 
    pub fn try_access(&self, path: &Path) -> Result<String, String> 
    pub fn id(&self) -> &str 
```

## How It Works

Read the source in `src/` for full implementation details. All algorithms are documented with inline comments explaining the mathematical foundations.

## The Math

This crate implements formal mathematical constructs. See the source documentation for theorem statements and proofs of correctness.

## Testing

**60 tests** covering construction, serialization, correctness properties, edge cases, and composability with other lau-* crates.

## License

MIT
