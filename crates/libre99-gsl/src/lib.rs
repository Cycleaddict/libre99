// Modified MIT License
//
// Copyright (c) 2026 Joel Odom
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, and sublicense copies of the
// Software, and to permit persons to whom the Software is furnished to do so,
// subject to the following conditions:
//
// "Commons Clause" License Condition v1.0
//
// The Software is provided to you by the Licensor under the License, subject to
// the following condition.
//
// Without limiting other conditions in the License, the grant of rights under the
// License will not include, and the License does not grant to you, the right to
// Sell the Software.
//
// For purposes of the foregoing, "Sell" means practicing any or all of the rights
// granted to you under the License to provide to third parties, for a fee or other
// consideration (including without limitation fees for hosting or consulting/
// support services related to the Software), a product or service whose value
// derives, entirely or substantially, from the functionality of the Software. Any
// license notice or attribution required by the License must also include this
// Commons Clause License Condition notice.
//
// Software: Libre99
//
// License: Modified MIT
//
// Licensor: Joel Odom
//
// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

//! # libre99-gsl — GSL, the GPL Structured Language
//!
//! A high-level language over TI-99/4A GPL bytecode, with a **compiler** and a
//! **self-verifying decompiler**. The language reference is `docs/GSL.md`; in
//! short:
//!
//! * [`codegen::compile`] turns `.gsl` source into a 64 KiB GROM-space image
//!   (plus ROM banks and container metadata) by lowering to `libre99gpl`
//!   assembler source and running the real assembler — inline `asm { }`
//!   blocks are therefore aligned with the standalone assembler by
//!   construction.
//! * [`decompile::decompile`] turns a `.ctg`/`.bin` image into GSL text and
//!   **verifies** it: the generated file recompiles byte-identically to the
//!   input payload (anything the tracer cannot express canonically survives
//!   as raw `data`/`BYTE` bytes), so decompiled output is functionally
//!   equivalent by construction.
//! * [`container`] normalizes the on-disk formats (`ti99sim` `.ctg` via
//!   `libre99-core`, raw GROM/ROM dumps) into payloads and back.

pub mod ast;
pub mod codegen;
pub mod container;
pub mod decompile;
pub mod fmtscan;
pub mod lexer;
pub mod parser;
pub mod wellknown;

pub use ast::OutFormat;
pub use codegen::{compile, Compiled, GslError};
pub use container::{parse_input, payload_of, write_output, Payload};
pub use decompile::{decompile, Decompiled, Options};
