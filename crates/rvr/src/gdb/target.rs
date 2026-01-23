//! GDB target implementation.
//!
//! Supports both RV32 and RV64 architectures with runtime dispatch.

use std::collections::HashSet;
use std::net::{TcpListener, TcpStream};

use gdbstub::common::Signal;
use gdbstub::conn::ConnectionExt;
use gdbstub::stub::run_blocking::{BlockingEventLoop, Event, WaitForStopReasonError};
use gdbstub::stub::{DisconnectReason, GdbStub, SingleThreadStopReason};
use gdbstub::target::ext::base::BaseOps;
use gdbstub::target::ext::base::singlethread::{
    SingleThreadBase, SingleThreadResume, SingleThreadResumeOps, SingleThreadSingleStep,
    SingleThreadSingleStepOps,
};
use gdbstub::target::ext::breakpoints::{
    Breakpoints, BreakpointsOps, SwBreakpoint, SwBreakpointOps,
};
use gdbstub::target::{Target, TargetResult};
use thiserror::Error;

use crate::runner::Runner;

/// GDB server error.
#[derive(Debug, Error)]
pub enum GdbError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("GDB stub error: {0}")]
    GdbStub(String),

    #[error("Runner error: {0}")]
    Runner(#[from] crate::runner::RunError),
}

/// Execution mode for the target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExecMode {
    /// Continue execution until breakpoint or exit.
    Continue,
    /// Execute a single instruction.
    Step,
}

/// Reason for stopping execution.
#[derive(Clone, Copy, Debug)]
#[allow(dead_code)] // DoneStep will be used when single-stepping is fully implemented
enum StopReason {
    /// Hit a software breakpoint.
    SwBreakpoint,
    /// Completed a single step.
    DoneStep,
    /// Program exited.
    Exited(u8),
}

/// Core GDB target state shared between RV32 and RV64 implementations.
struct GdbTargetCore {
    runner: Runner,
    breakpoints: HashSet<u64>,
    exec_mode: ExecMode,
}

impl GdbTargetCore {
    fn new(runner: Runner) -> Self {
        Self {
            runner,
            breakpoints: HashSet::new(),
            exec_mode: ExecMode::Continue,
        }
    }

    fn should_stop(&self) -> bool {
        self.breakpoints.contains(&self.runner.get_pc())
    }

    fn run_until_stop(&mut self) -> StopReason {
        // Check if already at a breakpoint
        if self.should_stop() {
            return StopReason::SwBreakpoint;
        }

        // For single-step mode, we need to execute one instruction
        // Currently we run the whole execution and stop at the end
        // TODO: Implement proper single-stepping with instret suspender
        if self.exec_mode == ExecMode::Step {
            let pc = self.runner.get_pc();
            match self.runner.execute_from(pc) {
                Ok(_) => StopReason::Exited(self.runner.exit_code()),
                Err(_) => StopReason::Exited(self.runner.exit_code()),
            }
        } else {
            let pc = self.runner.get_pc();
            match self.runner.execute_from(pc) {
                Ok(_) => {
                    if self.should_stop() {
                        StopReason::SwBreakpoint
                    } else {
                        StopReason::Exited(self.runner.exit_code())
                    }
                }
                Err(_) => StopReason::Exited(self.runner.exit_code()),
            }
        }
    }
}

// ============================================================================
// RV64 GDB Target
// ============================================================================

/// GDB target for RV64.
pub struct GdbTarget64 {
    core: GdbTargetCore,
}

impl GdbTarget64 {
    fn new(runner: Runner) -> Self {
        Self {
            core: GdbTargetCore::new(runner),
        }
    }
}

impl Target for GdbTarget64 {
    type Arch = gdbstub_arch::riscv::Riscv64;
    type Error = GdbError;

    fn base_ops(&mut self) -> BaseOps<'_, Self::Arch, Self::Error> {
        BaseOps::SingleThread(self)
    }

    fn support_breakpoints(&mut self) -> Option<BreakpointsOps<'_, Self>> {
        Some(self)
    }
}

impl SingleThreadBase for GdbTarget64 {
    fn read_registers(
        &mut self,
        regs: &mut gdbstub_arch::riscv::reg::RiscvCoreRegs<u64>,
    ) -> TargetResult<(), Self> {
        for i in 0..32 {
            regs.x[i] = self.core.runner.get_register(i);
        }
        regs.pc = self.core.runner.get_pc();
        Ok(())
    }

    fn write_registers(
        &mut self,
        regs: &gdbstub_arch::riscv::reg::RiscvCoreRegs<u64>,
    ) -> TargetResult<(), Self> {
        for i in 1..32 {
            self.core.runner.set_register(i, regs.x[i]);
        }
        self.core.runner.set_pc(regs.pc);
        Ok(())
    }

    fn read_addrs(&mut self, start_addr: u64, data: &mut [u8]) -> TargetResult<usize, Self> {
        let bytes_read = self.core.runner.read_memory(start_addr, data);
        Ok(bytes_read)
    }

    fn write_addrs(&mut self, start_addr: u64, data: &[u8]) -> TargetResult<(), Self> {
        self.core.runner.write_memory(start_addr, data);
        Ok(())
    }

    fn support_resume(&mut self) -> Option<SingleThreadResumeOps<'_, Self>> {
        Some(self)
    }
}

impl SingleThreadResume for GdbTarget64 {
    fn resume(&mut self, _signal: Option<Signal>) -> Result<(), Self::Error> {
        self.core.exec_mode = ExecMode::Continue;
        Ok(())
    }

    fn support_single_step(&mut self) -> Option<SingleThreadSingleStepOps<'_, Self>> {
        Some(self)
    }
}

impl SingleThreadSingleStep for GdbTarget64 {
    fn step(&mut self, _signal: Option<Signal>) -> Result<(), Self::Error> {
        self.core.exec_mode = ExecMode::Step;
        Ok(())
    }
}

impl Breakpoints for GdbTarget64 {
    fn support_sw_breakpoint(&mut self) -> Option<SwBreakpointOps<'_, Self>> {
        Some(self)
    }
}

impl SwBreakpoint for GdbTarget64 {
    fn add_sw_breakpoint(&mut self, addr: u64, _kind: usize) -> TargetResult<bool, Self> {
        self.core.breakpoints.insert(addr);
        Ok(true)
    }

    fn remove_sw_breakpoint(&mut self, addr: u64, _kind: usize) -> TargetResult<bool, Self> {
        Ok(self.core.breakpoints.remove(&addr))
    }
}

/// Blocking event loop for RV64 GDB.
struct GdbEventLoop64;

impl BlockingEventLoop for GdbEventLoop64 {
    type Target = GdbTarget64;
    type Connection = TcpStream;
    type StopReason = SingleThreadStopReason<u64>;

    fn wait_for_stop_reason(
        target: &mut GdbTarget64,
        conn: &mut Self::Connection,
    ) -> Result<Event<Self::StopReason>, WaitForStopReasonError<GdbError, std::io::Error>> {
        let mut poll_incoming_data = || conn.peek().map(|b| b.is_some()).unwrap_or(false);

        if poll_incoming_data() {
            let byte = conn.read().map_err(WaitForStopReasonError::Connection)?;
            return Ok(Event::IncomingData(byte));
        }

        match target.core.run_until_stop() {
            StopReason::SwBreakpoint => {
                Ok(Event::TargetStopped(SingleThreadStopReason::SwBreak(())))
            }
            StopReason::DoneStep => Ok(Event::TargetStopped(SingleThreadStopReason::DoneStep)),
            StopReason::Exited(code) => {
                Ok(Event::TargetStopped(SingleThreadStopReason::Exited(code)))
            }
        }
    }

    fn on_interrupt(_target: &mut GdbTarget64) -> Result<Option<Self::StopReason>, GdbError> {
        Ok(Some(SingleThreadStopReason::Signal(Signal::SIGINT)))
    }
}

// ============================================================================
// RV32 GDB Target
// ============================================================================

/// GDB target for RV32.
pub struct GdbTarget32 {
    core: GdbTargetCore,
}

impl GdbTarget32 {
    fn new(runner: Runner) -> Self {
        Self {
            core: GdbTargetCore::new(runner),
        }
    }
}

impl Target for GdbTarget32 {
    type Arch = gdbstub_arch::riscv::Riscv32;
    type Error = GdbError;

    fn base_ops(&mut self) -> BaseOps<'_, Self::Arch, Self::Error> {
        BaseOps::SingleThread(self)
    }

    fn support_breakpoints(&mut self) -> Option<BreakpointsOps<'_, Self>> {
        Some(self)
    }
}

impl SingleThreadBase for GdbTarget32 {
    fn read_registers(
        &mut self,
        regs: &mut gdbstub_arch::riscv::reg::RiscvCoreRegs<u32>,
    ) -> TargetResult<(), Self> {
        for i in 0..32 {
            regs.x[i] = self.core.runner.get_register(i) as u32;
        }
        regs.pc = self.core.runner.get_pc() as u32;
        Ok(())
    }

    fn write_registers(
        &mut self,
        regs: &gdbstub_arch::riscv::reg::RiscvCoreRegs<u32>,
    ) -> TargetResult<(), Self> {
        for i in 1..32 {
            self.core.runner.set_register(i, regs.x[i] as u64);
        }
        self.core.runner.set_pc(regs.pc as u64);
        Ok(())
    }

    fn read_addrs(&mut self, start_addr: u32, data: &mut [u8]) -> TargetResult<usize, Self> {
        let bytes_read = self.core.runner.read_memory(start_addr as u64, data);
        Ok(bytes_read)
    }

    fn write_addrs(&mut self, start_addr: u32, data: &[u8]) -> TargetResult<(), Self> {
        self.core.runner.write_memory(start_addr as u64, data);
        Ok(())
    }

    fn support_resume(&mut self) -> Option<SingleThreadResumeOps<'_, Self>> {
        Some(self)
    }
}

impl SingleThreadResume for GdbTarget32 {
    fn resume(&mut self, _signal: Option<Signal>) -> Result<(), Self::Error> {
        self.core.exec_mode = ExecMode::Continue;
        Ok(())
    }

    fn support_single_step(&mut self) -> Option<SingleThreadSingleStepOps<'_, Self>> {
        Some(self)
    }
}

impl SingleThreadSingleStep for GdbTarget32 {
    fn step(&mut self, _signal: Option<Signal>) -> Result<(), Self::Error> {
        self.core.exec_mode = ExecMode::Step;
        Ok(())
    }
}

impl Breakpoints for GdbTarget32 {
    fn support_sw_breakpoint(&mut self) -> Option<SwBreakpointOps<'_, Self>> {
        Some(self)
    }
}

impl SwBreakpoint for GdbTarget32 {
    fn add_sw_breakpoint(&mut self, addr: u32, _kind: usize) -> TargetResult<bool, Self> {
        self.core.breakpoints.insert(addr as u64);
        Ok(true)
    }

    fn remove_sw_breakpoint(&mut self, addr: u32, _kind: usize) -> TargetResult<bool, Self> {
        Ok(self.core.breakpoints.remove(&(addr as u64)))
    }
}

/// Blocking event loop for RV32 GDB.
struct GdbEventLoop32;

impl BlockingEventLoop for GdbEventLoop32 {
    type Target = GdbTarget32;
    type Connection = TcpStream;
    type StopReason = SingleThreadStopReason<u32>;

    fn wait_for_stop_reason(
        target: &mut GdbTarget32,
        conn: &mut Self::Connection,
    ) -> Result<Event<Self::StopReason>, WaitForStopReasonError<GdbError, std::io::Error>> {
        let mut poll_incoming_data = || conn.peek().map(|b| b.is_some()).unwrap_or(false);

        if poll_incoming_data() {
            let byte = conn.read().map_err(WaitForStopReasonError::Connection)?;
            return Ok(Event::IncomingData(byte));
        }

        match target.core.run_until_stop() {
            StopReason::SwBreakpoint => {
                Ok(Event::TargetStopped(SingleThreadStopReason::SwBreak(())))
            }
            StopReason::DoneStep => Ok(Event::TargetStopped(SingleThreadStopReason::DoneStep)),
            StopReason::Exited(code) => {
                Ok(Event::TargetStopped(SingleThreadStopReason::Exited(code)))
            }
        }
    }

    fn on_interrupt(_target: &mut GdbTarget32) -> Result<Option<Self::StopReason>, GdbError> {
        Ok(Some(SingleThreadStopReason::Signal(Signal::SIGINT)))
    }
}

// ============================================================================
// GDB Server - runtime dispatch between RV32 and RV64
// ============================================================================

/// GDB server with runtime architecture dispatch.
pub struct GdbServer {
    runner: Runner,
}

impl GdbServer {
    /// Create a new GDB server wrapping a runner.
    pub fn new(runner: Runner) -> Self {
        Self { runner }
    }

    /// Run the GDB server, blocking until the client disconnects.
    ///
    /// `addr` should be in the format `:port` or `host:port`.
    pub fn run(self, addr: &str) -> Result<(), GdbError> {
        // Parse address - if it starts with ":", prepend "127.0.0.1"
        let addr = if addr.starts_with(':') {
            format!("127.0.0.1{}", addr)
        } else {
            addr.to_string()
        };

        // Bind and wait for connection
        let listener = TcpListener::bind(&addr)?;
        eprintln!("Waiting for GDB connection on {}...", addr);

        let (stream, peer) = listener.accept()?;
        eprintln!("GDB connected from {}", peer);

        // Dispatch based on XLEN
        let xlen = self.runner.xlen();
        eprintln!("Target architecture: RV{}", xlen);

        if xlen == 32 {
            self.run_rv32(stream)
        } else {
            self.run_rv64(stream)
        }
    }

    fn run_rv64(self, stream: TcpStream) -> Result<(), GdbError> {
        let mut target = GdbTarget64::new(self.runner);
        target.core.runner.prepare();

        let gdb = GdbStub::new(stream);
        match gdb.run_blocking::<GdbEventLoop64>(&mut target) {
            Ok(reason) => Self::handle_disconnect(reason),
            Err(e) => Err(GdbError::GdbStub(format!("{:?}", e))),
        }
    }

    fn run_rv32(self, stream: TcpStream) -> Result<(), GdbError> {
        let mut target = GdbTarget32::new(self.runner);
        target.core.runner.prepare();

        let gdb = GdbStub::new(stream);
        match gdb.run_blocking::<GdbEventLoop32>(&mut target) {
            Ok(reason) => Self::handle_disconnect(reason),
            Err(e) => Err(GdbError::GdbStub(format!("{:?}", e))),
        }
    }

    fn handle_disconnect(reason: DisconnectReason) -> Result<(), GdbError> {
        eprintln!("GDB session ended: {:?}", reason);
        match reason {
            DisconnectReason::Disconnect => Ok(()),
            DisconnectReason::TargetExited(code) => {
                eprintln!("Target exited with code {}", code);
                Ok(())
            }
            DisconnectReason::TargetTerminated(sig) => {
                eprintln!("Target terminated with signal {:?}", sig);
                Ok(())
            }
            DisconnectReason::Kill => {
                eprintln!("Target killed by GDB");
                Ok(())
            }
        }
    }
}
