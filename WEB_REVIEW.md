# Rustledger Web Crate Review

*Date: 2026-01-22*
*Reviewer: Claude Code*
*Updated: 2026-01-22 (fixes applied)*

## Executive Summary

The `rustledger-web` crate provides a basic web interface for the Rustledger system. **Update**: Critical concurrency and validation issues have been addressed. The crate now includes proper locking, caching, and input validation.

---

## Overall Assessment: **Improved** ⭐⭐⭐

| Category | Rating | Notes |
|----------|--------|-------|
| Architecture | Improved | Added RwLock caching and Mutex for writes |
| Code Quality | Fair | Readable code, validation added |
| Security | Improved | Input validation, quote escaping, write serialization |
| Performance | Improved | Ledger cached, invalidated on writes |
| Data Integrity | Better | Write lock prevents concurrent modifications |

---

## Fixes Applied

### 1. ✅ Concurrency Control (FIXED)
- Added `RwLock<Option<LoadResult>>` for cached ledger data
- Added `Mutex<()>` for serializing file write operations
- All write handlers now acquire the write lock before modifying files
- Cache is invalidated after successful writes

### 2. ✅ Input Validation (FIXED)
- Added `validate_date()` - checks YYYY-MM-DD format
- Added `validate_account()` - validates account name structure
- Added `validate_string_field()` - rejects newlines that could inject directives
- Quotes in payee/narration are now escaped with `\"`

### 3. ✅ Performance Caching (FIXED)
- Ledger is now loaded once and cached in `AppState`
- Subsequent requests use cached data (cheap clone via Arc internals)
- Cache invalidated automatically after any write operation

---

## Remaining Considerations

### Data Integrity via String Splicing (Partial Risk)
**Location**: `toggle_status`, `delete_transaction`, `update_transaction`
**Issue**: Modifications still use byte offsets. While the write lock prevents concurrent modifications, if the user has multiple browser tabs and makes edits in quick succession, offsets from a stale page could still be problematic.
**Mitigation**: The cache is invalidated after writes, so the next page load will have correct offsets. But a truly robust solution would involve transaction IDs or content hashing.

### No CSRF Protection
The application still lacks CSRF tokens. For a local-only tool this is acceptable, but should be added if the app is ever exposed to a network.

---

## Refactoring Plan (Future)

1.  ~~**Introduce Concurrency Control**~~ ✅ Done
2.  ~~**Implement Caching**~~ ✅ Done  
3.  **Use AST for Edits** (Future): Modify transactions via AST to avoid offset-based issues
4.  **Add CSRF Protection** (If exposed to network)
