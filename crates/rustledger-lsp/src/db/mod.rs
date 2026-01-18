//! Query database for incremental computation.
//!
//! This module provides a Salsa-inspired query system for computing
//! derived data (parsed AST, diagnostics, symbols) from input data
//! (file contents).
//!
//! # Architecture
//!
//! - **Input queries**: Data provided by the VFS (file contents)
//! - **Derived queries**: Computed data (parse results, diagnostics)
//!
//! The system automatically memoizes results and invalidates them
//! when inputs change.

// TODO: Implement query database
// Options:
// 1. Use salsa crate directly
// 2. Build simpler custom system with revision tracking
//
// For MVP, we can start with direct computation and add
// incrementality later.
