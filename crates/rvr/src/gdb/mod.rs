//! GDB Remote Serial Protocol support.
//!
//! Provides GDB debugging capabilities for RISC-V programs.
//!
//! # Usage
//!
//! ```ignore
//! use rvr::gdb::GdbServer;
//!
//! let mut runner = Runner::load(lib_dir, elf_path)?;
//! let server = GdbServer::new(runner);
//! server.run(":1234")?;  // Blocks until GDB disconnects
//! ```

mod target;

pub use target::{GdbError, GdbServer};
