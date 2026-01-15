# Rustledger Performance Optimization Roadmap

## Current Performance

| Metric | Value |
|--------|-------|
| rustledger | 160ms |
| Python beancount | 863ms |
| Current speedup | **5.4x** |

## Target

Push the speedup from 5x to **10-20x** through systematic optimization.

---

## Measured Results

| Change | Before | After | Improvement |
|--------|--------|-------|-------------|
| Phase 0.1: Arc<str> | 160ms | 134ms | **16% faster** |

---

## Phase 0: Quick Wins (Day 1)

**Goal**: Low-effort, high-impact changes
**Expected Impact**: 15-25% faster

### 0.1 Eliminate Source Code Double Allocation
- **File**: `crates/rustledger-loader/src/lib.rs`
- **Line**: 308
- **Problem**: `fs::read_to_string()` then `source.clone()` = 2x memory
- **Fix**: Use `Arc<str>` instead of cloning
```rust
// Before
let source = fs::read_to_string(path)?;
source_map.add_file(path, source.clone());  // CLONE!

// After
let source: Arc<str> = fs::read_to_string(path)?.into();
source_map.add_file(path, Arc::clone(&source));  // Cheap refcount
```
- **Impact**: 50% reduction in source memory, faster loading

### 0.2 Enable Profile-Guided Optimization (PGO)
- **File**: `.cargo/config.toml` (new), `.github/workflows/release.yml`
- **Change**: Build release binaries with PGO data from benchmarks
- **Impact**: 5-15% overall speedup (free optimization)

---

## Phase 1: Parser Allocation Fixes (Week 1)

**Goal**: Eliminate unnecessary allocations in the parser
**Expected Impact**: 20-30% faster

### 1.1 Zero-Copy String Parsing
- **File**: `crates/rustledger-parser/src/parser.rs`
- **Lines**: 622, 886, 922, 934, 942
- **Problem**: Parser calls `.to_string()` on slices that could stay borrowed
- **Fix**: Return `&'a str` instead of `String`, intern at directive construction
```rust
// Before
.map(|s: &str| s.to_string())  // Allocates!

// After
.map(|s: &str| s)  // Zero-copy, intern later
```
- **Impact**: ~15% parsing improvement

### 1.2 Fix Vector Cloning
- **File**: `crates/rustledger-parser/src/parser.rs`
- **Lines**: 1055, 1080
- **Change**: Use `.into_iter()` instead of `.clone().into_iter()`
- **Impact**: ~5% improvement

### 1.3 Use Rc for Metadata in Closures
- **File**: `crates/rustledger-parser/src/parser.rs`
- **Lines**: 1271, 1305, 1329, etc.
- **Change**: Wrap metadata in `Rc<Metadata>` to avoid cloning
- **Impact**: ~10% improvement

---

## Phase 2: Collection Optimizations (Week 2)

**Goal**: Reduce heap allocations for small collections
**Expected Impact**: 15-25% faster

### 2.1 Add SmallVec Dependency
```toml
# crates/rustledger-core/Cargo.toml
smallvec = "1.11"
```

### 2.2 Convert Small Vectors
```rust
// crates/rustledger-core/src/directive.rs
pub tags: SmallVec<[InternedStr; 4]>,    // was Vec<String>
pub links: SmallVec<[InternedStr; 2]>,   // was Vec<String>
pub postings: SmallVec<[Posting; 4]>,    // was Vec<Posting>
```

### 2.3 Pre-allocate HashMaps
- Add `.with_capacity()` calls in validation and query execution
- **Files**: `rustledger-validate/src/lib.rs`, `rustledger-query/src/executor.rs`

---

## Phase 3: String Interning (Week 3-4)

**Goal**: Deduplicate strings across entire ledger
**Expected Impact**: 10-20% faster, 30-50% less memory

### 3.1 Extend InternedStr Usage
```rust
// crates/rustledger-core/src/directive.rs
pub struct Transaction {
    pub payee: Option<InternedStr>,    // was Option<String>
    pub narration: InternedStr,        // was String
    pub tags: SmallVec<[InternedStr; 4]>,
    pub links: SmallVec<[InternedStr; 2]>,
}
```

### 3.2 Intern at Parse Time
- Pass `StringInterner` to parser
- Intern strings immediately when parsed
- Share interner across all parsed files

---

## Phase 4: Parallelization (Week 5-6)

**Goal**: Use multiple CPU cores
**Expected Impact**: 2-4x faster on multi-core
**Breaking Changes**: None (internal)

### 4.1 Add Rayon Dependency
```toml
# crates/rustledger-validate/Cargo.toml
rayon = "1.8"
```

### 4.2 Parallel Transaction Processing
- Interpolate transactions in parallel
- Validate independent checks in parallel
- Keep sorting single-threaded (required for correctness)

---

## Phase 5: Binary Cache Format (Week 5-6)

**Goal**: Cache parsed ledgers for instant reload
**Expected Impact**: 50-90% faster on cache hit

### 5.1 Implement Cache Format
- **File**: `crates/rustledger-loader/src/cache.rs` (new)
- **Format**: [rkyv](https://github.com/rkyv/rkyv) for zero-copy deserialization (faster than bincode)
- **Cache key**: SHA256 hash of source file content
- **Location**: `ledger.beancount` → `ledger.beancount.cache`

Why rkyv over bincode:
- Zero-copy: access data directly from mmap'd cache file
- [Benchmarks show](https://david.kolo.ski/blog/rkyv-is-faster-than/) rkyv wins nearly every performance category
- No deserialization step needed - just validate and use

### 5.2 Cache Invalidation
- Check source file hash on load
- If hash matches cache → instant load
- If hash differs → parse and rebuild cache

### 5.3 Optional Flag
```bash
rledger-check --no-cache ledger.beancount  # Skip cache
rledger-check ledger.beancount             # Use cache (default)
```

---

## Phase 6: Lexer + Arena Allocator (Future)

**Goal**: Replace parser combinators with fast lexer, use arena for AST
**Expected Impact**: 30-50% faster parsing

### 6.1 Logos Lexer + Chumsky Parser via logosky
- Use [logos](https://github.com/maciejhirsz/logos) crate for SIMD-accelerated tokenization
- Use [logosky](https://crates.io/crates/logosky) to bridge Logos output to Chumsky
- Zero-copy token stream - no allocations during lexing
- Enable existing `lexer.rs` (currently disabled)

### 6.2 Bumpalo Arena for AST Nodes
- Use [bumpalo](https://github.com/fitzgen/bumpalo) for AST allocation
- Only 11 instructions per allocation (vs ~100 for malloc)
- Mass deallocation: just reset the bump pointer
- Perfect for phase-oriented allocation (parse → use → discard)

---

## Phase 7: Memory-Mapped Files (Future)

**Goal**: Zero-copy file loading for very large ledgers
**Expected Impact**: 10-20% for files >100MB

### 7.1 Optional mmap for Large Files
- Only enable for files > threshold (e.g., 50MB)
- Fallback to standard read for smaller files
- Cross-platform support (memmap2 crate)

---

## Roadmap Summary

| Phase | Work | Impact | Timeline |
|-------|------|--------|----------|
| 0 | Quick wins (Arc, PGO) | +20% | Day 1 |
| 1 | Zero-copy parsing | +25% | Week 1 |
| 2 | SmallVec + InternedStr | +20% | Week 2 |
| 3 | Full interning | +15% | Week 3-4 |
| 4 | Parallelization (rayon) | +100% | Week 4-5 |
| 5 | Binary cache (rkyv) | +50-90%* | Week 5-6 |
| 6 | Logos + Bumpalo | +40% | Future |
| 7 | Memory-mapped files | +10-20%** | Future |

*On cache hit only
**For files >100MB only

## Projected Performance

| After Phase | Speedup vs Python |
|-------------|-------------------|
| Current | 5.4x |
| Phase 0 | ~6-7x |
| Phase 1 | ~8-9x |
| Phase 2 | ~10-11x |
| Phase 3 | ~12x |
| Phase 4 | ~20-25x |
| Phase 5 | instant* |
| Phase 6 | ~30x+ |

*Cache hit = sub-10ms load for any size ledger

---

## Measurement Plan

Each phase should be benchmarked:

```bash
# Before/after each phase
cargo bench --bench pipeline_bench

# Nightly CI comparison (already set up)
# Results in benchmarks branch
```

---

## Decision Points

1. **After Phase 0**: Measure baseline improvement before deeper work
2. **After Phase 3**: Evaluate if 12x is sufficient or continue to parallelization
3. **Phase 5 (Cache)**: High value for development workflows, optional for CI
4. **Phase 6-7**: Only pursue if profiling shows remaining bottlenecks

---

## Research & References

### Parser Performance
- [Winnow](https://epage.github.io/blog/2023/07/winnow-0-5-the-fastest-rust-parser-combinator-library/) - potentially faster than Chumsky for some use cases
- [Chumsky](https://github.com/zesterer/chumsky) - current parser, good error recovery
- [logosky](https://crates.io/crates/logosky) - zero-copy bridge from Logos to Chumsky

### Serialization
- [rkyv](https://github.com/rkyv/rkyv) - zero-copy deserialization, [faster than bincode](https://david.kolo.ski/blog/rkyv-is-faster-than/)
- [rust_serialization_benchmark](https://github.com/djkoloski/rust_serialization_benchmark) - comprehensive comparison

### Memory Management
- [bumpalo](https://github.com/fitzgen/bumpalo) - fast arena allocator (11 instructions/alloc)
- [Guide to arenas in Rust](https://blog.logrocket.com/guide-using-arenas-rust/)

### String Processing
- [memchr](https://github.com/BurntSushi/memchr) - SIMD-accelerated string search
- [aho-corasick](https://github.com/BurntSushi/aho-corasick) - SIMD multi-pattern matching

### Compiler Optimizations
- [PGO in Rust](https://doc.rust-lang.org/rustc/profile-guided-optimization.html) - 10-30% improvement
- [Rust compiler performance 2025](https://blog.rust-lang.org/2025/09/10/rust-compiler-performance-survey-2025-results/) - 6x faster builds
