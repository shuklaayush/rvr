//! GDB target implementation.

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

/// GDB target wrapping a Runner.
pub struct GdbTarget {
    runner: Runner,
    breakpoints: HashSet<u64>,
    exec_mode: ExecMode,
}

impl GdbTarget {
    /// Create a new GDB target.
    pub fn new(runner: Runner) -> Self {
        Self {
            runner,
            breakpoints: HashSet::new(),
            exec_mode: ExecMode::Continue,
        }
    }

    /// Check if we should stop at the current PC.
    fn should_stop(&self) -> bool {
        self.breakpoints.contains(&self.runner.get_pc())
    }

    /// Execute until a stop condition is met.
    fn run_until_stop(&mut self) -> StopReason {
        // Check if already at a breakpoint
        if self.should_stop() {
            return StopReason::SwBreakpoint;
        }

        // For single-step mode, we need to execute one instruction
        // Currently we run the whole execution and stop at the end
        // TODO: Implement proper single-stepping with instret suspender
        if self.exec_mode == ExecMode::Step {
            // Execute from current PC
            let pc = self.runner.get_pc();
            match self.runner.execute_from(pc) {
                Ok(_) => StopReason::Exited(self.runner.exit_code()),
                Err(_) => StopReason::Exited(self.runner.exit_code()),
            }
        } else {
            // Continue mode - run until exit
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

// Implement the Target trait for RV64
impl Target for GdbTarget {
    type Arch = gdbstub_arch::riscv::Riscv64;
    type Error = GdbError;

    fn base_ops(&mut self) -> BaseOps<'_, Self::Arch, Self::Error> {
        BaseOps::SingleThread(self)
    }

    fn support_breakpoints(&mut self) -> Option<BreakpointsOps<'_, Self>> {
        Some(self)
    }
}

impl SingleThreadBase for GdbTarget {
    fn read_registers(
        &mut self,
        regs: &mut gdbstub_arch::riscv::reg::RiscvCoreRegs<u64>,
    ) -> TargetResult<(), Self> {
        // Read x0-x31
        for i in 0..32 {
            regs.x[i] = self.runner.get_register(i);
        }
        // Read PC
        regs.pc = self.runner.get_pc();
        Ok(())
    }

    fn write_registers(
        &mut self,
        regs: &gdbstub_arch::riscv::reg::RiscvCoreRegs<u64>,
    ) -> TargetResult<(), Self> {
        // Write x1-x31 (x0 is hardwired to zero)
        for i in 1..32 {
            self.runner.set_register(i, regs.x[i]);
        }
        // Write PC
        self.runner.set_pc(regs.pc);
        Ok(())
    }

    fn read_addrs(&mut self, start_addr: u64, data: &mut [u8]) -> TargetResult<usize, Self> {
        let bytes_read = self.runner.read_memory(start_addr, data);
        Ok(bytes_read)
    }

    fn write_addrs(&mut self, start_addr: u64, data: &[u8]) -> TargetResult<(), Self> {
        self.runner.write_memory(start_addr, data);
        Ok(())
    }

    fn support_resume(&mut self) -> Option<SingleThreadResumeOps<'_, Self>> {
        Some(self)
    }
}

impl SingleThreadResume for GdbTarget {
    fn resume(&mut self, _signal: Option<Signal>) -> Result<(), Self::Error> {
        self.exec_mode = ExecMode::Continue;
        Ok(())
    }

    fn support_single_step(&mut self) -> Option<SingleThreadSingleStepOps<'_, Self>> {
        Some(self)
    }
}

impl SingleThreadSingleStep for GdbTarget {
    fn step(&mut self, _signal: Option<Signal>) -> Result<(), Self::Error> {
        self.exec_mode = ExecMode::Step;
        Ok(())
    }
}

impl Breakpoints for GdbTarget {
    fn support_sw_breakpoint(&mut self) -> Option<SwBreakpointOps<'_, Self>> {
        Some(self)
    }
}

impl SwBreakpoint for GdbTarget {
    fn add_sw_breakpoint(&mut self, addr: u64, _kind: usize) -> TargetResult<bool, Self> {
        self.breakpoints.insert(addr);
        Ok(true)
    }

    fn remove_sw_breakpoint(&mut self, addr: u64, _kind: usize) -> TargetResult<bool, Self> {
        Ok(self.breakpoints.remove(&addr))
    }
}

/// Blocking event loop for GDB.
struct GdbEventLoop;

impl BlockingEventLoop for GdbEventLoop {
    type Target = GdbTarget;
    type Connection = TcpStream;
    type StopReason = SingleThreadStopReason<u64>;

    fn wait_for_stop_reason(
        target: &mut GdbTarget,
        conn: &mut Self::Connection,
    ) -> Result<Event<Self::StopReason>, WaitForStopReasonError<GdbError, std::io::Error>> {
        // Poll for incoming data from GDB (e.g., Ctrl+C interrupt)
        let mut poll_incoming_data = || {
            // Use ConnectionExt::peek to check for available data
            conn.peek().map(|b| b.is_some()).unwrap_or(false)
        };

        // Check if GDB sent us data (e.g., Ctrl+C)
        if poll_incoming_data() {
            let byte = conn.read().map_err(WaitForStopReasonError::Connection)?;
            return Ok(Event::IncomingData(byte));
        }

        // Run the target until stop
        match target.run_until_stop() {
            StopReason::SwBreakpoint => {
                Ok(Event::TargetStopped(SingleThreadStopReason::SwBreak(())))
            }
            StopReason::DoneStep => Ok(Event::TargetStopped(SingleThreadStopReason::DoneStep)),
            StopReason::Exited(code) => {
                Ok(Event::TargetStopped(SingleThreadStopReason::Exited(code)))
            }
        }
    }

    fn on_interrupt(_target: &mut GdbTarget) -> Result<Option<Self::StopReason>, GdbError> {
        Ok(Some(SingleThreadStopReason::Signal(Signal::SIGINT)))
    }
}

/// GDB server.
pub struct GdbServer {
    target: GdbTarget,
}

impl GdbServer {
    /// Create a new GDB server wrapping a runner.
    pub fn new(runner: Runner) -> Self {
        Self {
            target: GdbTarget::new(runner),
        }
    }

    /// Run the GDB server, blocking until the client disconnects.
    ///
    /// `addr` should be in the format `:port` or `host:port`.
    pub fn run(&mut self, addr: &str) -> Result<(), GdbError> {
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

        // Prepare the target
        self.target.runner.prepare();

        // Create and run GDB stub
        let gdb = GdbStub::new(stream);

        match gdb.run_blocking::<GdbEventLoop>(&mut self.target) {
            Ok(reason) => {
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
            Err(e) => Err(GdbError::GdbStub(format!("{:?}", e))),
        }
    }
}
