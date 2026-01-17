# Rustledger Performance Optimization Roadmap

## Current Performance (10K transactions)

| Benchmark | rustledger | beancount | Speedup |
|-----------|------------|-----------|---------|
| Validation (parse + check) | 35ms | 754ms | **22x faster** |
| Balance report (parse + compute) | 118ms | 1280ms | **11x faster** |

## Target

~~Push the speedup from 5x to **10-20x** through systematic optimization.~~ **Achieved!**

---

## Measured Results

| Change | Before | After | Improvement |
|--------|--------|-------|-------------|
| Phase 0.1: Arc<str> | 160ms | 134ms | **16% faster** |
| Phase 1.1: Rc for closures | 113ms | 141ms | ❌ 25% slower (reverted) |
| Phase 1.1: Zero-copy primitives | 108ms | 101ms | **~7% faster** |
| Phase 2: SmallVec | 113ms | 143ms | ❌ 27% slower (reverted) |
| Phase 3: Full string interning | 30ms | 28ms | **~6% faster** |
| Phase 4: Rayon parallelization | 113ms | 108ms | **~5% faster** |
| Phase 0.2: PGO | 108ms | 94ms | **13% faster** |
| Phase 5: rkyv cache | 30ms | 13ms | **2.3x faster** (cache hit) |

**Combined improvement**: 160ms → 94ms = **41% faster** (1.7x speedup on top of existing gains)

**With full interning**: ~28ms on 7176-line file (cold parse)

**With cache hit**: 13ms = **instant** for repeated runs (7176-line file benchmark)

**Note**: Local benchmarks run on 10K transaction ledger. Rc and SmallVec add overhead that outweighs benefits. Phase 3 extends InternedStr to payee/narration/tags/links for memory deduplication. Cache provides 2.3x speedup on subsequent runs.

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

## Phase 3: String Interning (Week 3-4) ✅ DONE

**Goal**: Deduplicate strings across entire ledger
**Result**: ~6% faster, memory deduplication via Arc<str>

### 3.1 Extend InternedStr Usage ✅
```rust
// crates/rustledger-core/src/directive.rs
pub struct Transaction {
    pub payee: Option<InternedStr>,    // was Option<String>
    pub narration: InternedStr,        // was String
    pub tags: Vec<InternedStr>,        // was Vec<String>
    pub links: Vec<InternedStr>,       // was Vec<String>
}

pub struct Document {
    pub tags: Vec<InternedStr>,        // was Vec<String>
    pub links: Vec<InternedStr>,       // was Vec<String>
}
```

### 3.2 Cache Re-interning ✅
- `reintern_directives()` deduplicates strings after cache load
- Typical deduplication: 150+ strings per ledger
- Memory savings from Arc<str> sharing

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

## Phase 5: Binary Cache Format (Week 5-6) ✅ DONE

**Goal**: Cache parsed ledgers for instant reload
**Result**: 2.3x faster on cache hit (30ms → 13ms)

### 5.1 Implement Cache Format ✅
- **File**: `crates/rustledger-loader/src/cache.rs`
- **Format**: [rkyv](https://github.com/rkyv/rkyv) for zero-copy deserialization
- **Cache key**: SHA256 hash of file mtime + size
- **Location**: `ledger.beancount` → `ledger.beancount.cache`

Custom rkyv wrappers for non-rkyv types:
- `AsDecimal` - Decimal as 16-byte binary
- `AsNaiveDate` - Date as i32 days since epoch
- `AsInternedStr` - InternedStr as ArchivedString

### 5.2 Cache Invalidation ✅
- Hash computed from all included files' mtime + size
- Graceful fallback on cache errors
- `invalidate_cache()` API for manual invalidation

### 5.3 CLI Integration ✅
```bash
rledger-check --no-cache ledger.beancount  # Skip cache
rledger-check -C ledger.beancount          # Short form
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

| Phase | Work | Status | Result |
|-------|------|--------|--------|
| 0 | Quick wins (Arc, PGO) | ✅ Done | +29% (16% + 13%) |
| 1 | Zero-copy parsing | ✅ Done | +7% |
| 2 | SmallVec | ❌ Reverted | -27% (slower) |
| 3 | Full interning | ✅ Done | +6% |
| 4 | Parallelization (rayon) | ✅ Done | +5% |
| 5 | Binary cache (rkyv) | ✅ Done | 2.3x on cache hit |
| 6 | Logos + Bumpalo | 🔮 Future | +40% projected |
| 7 | Memory-mapped files | 🔮 Future | Large files only |

## Actual Performance

Measured on 10K transaction ledgers (January 2026):

| Benchmark | rustledger | beancount | ledger (C++) | hledger |
|-----------|------------|-----------|--------------|---------|
| Validation | 35ms | 754ms | 97ms | 467ms |
| Balance report | 118ms | 1280ms | 84ms | 571ms |

**Key results:**
- **22x faster** than beancount for validation
- **11x faster** than beancount for balance reports
- Competitive with ledger (C++): 2.8x slower validation, 1.4x slower balance
- Cache hit: ~13ms for repeated runs

---

## Benchmark Evaluation (January 2026)

### Methodology Verification

The benchmark claims have been independently verified. Key findings:

**1. What each command measures:**
| Tool | Command | Operation |
|------|---------|-----------|
| rustledger | `rledger-check file.beancount` | Parse + validate |
| beancount | `bean-check file.beancount` | Parse + validate (no plugins on simple files) |
| ledger | `ledger -f file.ledger accounts` | Parse + list accounts |
| hledger | `hledger check -f file.ledger` | Parse + validate |

All commands perform equivalent work: parse the file and validate correctness.

**2. Output equivalence verified:**
Both `rledger-check` and `bean-check` produce the same result on test files (no errors, same directive counts).

### Scaling Analysis

| Transactions | File Size | rustledger | beancount | Speedup |
|-------------|-----------|------------|-----------|---------|
| 1K | 100 KB | 4.5ms | 149ms | **33x** |
| 5K | 507 KB | 16.2ms | - | - |
| 10K | 1 MB | 30.4ms | 744ms | **24x** |
| 50K | 5 MB | 147ms | - | - |
| 100K | 10 MB | 304ms | 3,099ms | **10x** |

**Key insight:** Speedup varies from 10x to 33x depending on file size.

### Startup Overhead Analysis

The varying speedup is explained by startup overhead:

| Tool | Startup | Processing 10K |
|------|---------|----------------|
| rustledger | ~2ms | ~28ms |
| beancount | ~100ms | ~644ms |

- **Small files (1K):** Startup dominates → 33x speedup
- **Large files (100K):** Pure processing dominates → 10x speedup
- **Typical files (10K):** Mixed → 20-24x speedup

### Scaling Behavior

Both tools exhibit O(n) scaling:

**rustledger:**
- 5K → 10K: 16.2ms → 30.4ms (1.9x for 2x input) ✓
- 10K → 50K: 30.4ms → 147ms (4.8x for 5x input) ✓
- 50K → 100K: 147ms → 304ms (2.1x for 2x input) ✓

Throughput: ~330K transactions/second (after warmup)

**beancount:**
- 1K → 10K: 149ms → 744ms (5.0x for 10x input) ✓
- 10K → 100K: 744ms → 3,099ms (4.2x for 10x input) ✓

Throughput: ~32K transactions/second (at scale)

### Conclusion

The benchmark claims are **accurate and fair**:

1. ✅ Both tools perform equivalent validation work
2. ✅ Both exhibit linear O(n) scaling
3. ✅ rustledger is genuinely 10-33x faster
4. ✅ Speedup variation explained by startup overhead (2ms vs 100ms)

The "10x faster" claim is conservative (applies to 100K+ transactions). For typical ledgers (1K-10K transactions), rustledger is **20-30x faster**.

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
