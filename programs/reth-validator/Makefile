# Makefile for reth-validator - RISC-V Compilation (RVR)
#
# Builds reth-validator for multiple RISC-V targets and outputs to bin/reth/

# Use specific nightly to avoid compiler_builtins regression
NIGHTLY = nightly-2025-12-05

# Linker script - force relink when changed
LINKER_SCRIPT = link.x

# Output root (relative to this Makefile)
BIN_ROOT = ../../bin/reth

# RV32I Target
TARGET32 = rv32i.json
OUTPUT_DIR32 = $(BIN_ROOT)/rv32i
RUSTFLAGS32 = -Clink-arg=-Tlink.x -Clink-arg=--gc-sections -Ctarget-cpu=generic-rv32 -Ccode-model=medium -Zunstable-options -Cpanic=immediate-abort

# RV32E Target (16 registers)
TARGET32E = rv32e.json
OUTPUT_DIR32E = $(BIN_ROOT)/rv32e
RUSTFLAGS32E = -Clink-arg=-Tlink.x -Clink-arg=--gc-sections -Ctarget-cpu=generic-rv32 -Ccode-model=medium -Zunstable-options -Cpanic=immediate-abort

# RV64I Target
TARGET64 = rv64i.json
OUTPUT_DIR64 = $(BIN_ROOT)/rv64i
RUSTFLAGS64 = -Clink-arg=-Tlink.x -Clink-arg=--gc-sections -Ctarget-cpu=generic-rv64 -Ccode-model=medium -Zunstable-options -Cpanic=immediate-abort

# RV64E Target (16 registers)
TARGET64E = rv64e.json
OUTPUT_DIR64E = $(BIN_ROOT)/rv64e
RUSTFLAGS64E = -Clink-arg=-Tlink.x -Clink-arg=--gc-sections -Ctarget-cpu=generic-rv64 -Ccode-model=medium -Zunstable-options -Cpanic=immediate-abort

BINARY = reth-validator

all: rv32i rv32e rv64i rv64e

$(OUTPUT_DIR32):
	mkdir -p $(OUTPUT_DIR32)

$(OUTPUT_DIR32E):
	mkdir -p $(OUTPUT_DIR32E)

$(OUTPUT_DIR64):
	mkdir -p $(OUTPUT_DIR64)

$(OUTPUT_DIR64E):
	mkdir -p $(OUTPUT_DIR64E)

# Force relink if linker script is newer than cargo's output
rv32i: | $(OUTPUT_DIR32)
	@test ! -f target/rv32i/release/$(BINARY) -o $(LINKER_SCRIPT) -nt target/rv32i/release/$(BINARY) && rm -f target/rv32i/release/$(BINARY) || true
	RUSTFLAGS="$(RUSTFLAGS32)" cargo +$(NIGHTLY) build --release --target $(TARGET32) -Zbuild-std=core,alloc -Zbuild-std-features=compiler-builtins-mem
	cp target/rv32i/release/$(BINARY) $(OUTPUT_DIR32)/$(BINARY)

rv32e: | $(OUTPUT_DIR32E)
	@test ! -f target/rv32e/release/$(BINARY) -o $(LINKER_SCRIPT) -nt target/rv32e/release/$(BINARY) && rm -f target/rv32e/release/$(BINARY) || true
	RUSTFLAGS="$(RUSTFLAGS32E)" cargo +$(NIGHTLY) build --release --target $(TARGET32E) -Zbuild-std=core,alloc -Zbuild-std-features=compiler-builtins-mem
	cp target/rv32e/release/$(BINARY) $(OUTPUT_DIR32E)/$(BINARY)

rv64i: | $(OUTPUT_DIR64)
	@test ! -f target/rv64i/release/$(BINARY) -o $(LINKER_SCRIPT) -nt target/rv64i/release/$(BINARY) && rm -f target/rv64i/release/$(BINARY) || true
	RUSTFLAGS="$(RUSTFLAGS64)" cargo +$(NIGHTLY) build --release --target $(TARGET64) -Zbuild-std=core,alloc -Zbuild-std-features=compiler-builtins-mem
	cp target/rv64i/release/$(BINARY) $(OUTPUT_DIR64)/$(BINARY)

rv64e: | $(OUTPUT_DIR64E)
	@test ! -f target/rv64e/release/$(BINARY) -o $(LINKER_SCRIPT) -nt target/rv64e/release/$(BINARY) && rm -f target/rv64e/release/$(BINARY) || true
	RUSTFLAGS="$(RUSTFLAGS64E)" cargo +$(NIGHTLY) build --release --target $(TARGET64E) -Zbuild-std=core,alloc -Zbuild-std-features=compiler-builtins-mem
	cp target/rv64e/release/$(BINARY) $(OUTPUT_DIR64E)/$(BINARY)

# Build for host (native baseline)
host:
	cargo build --release

clean:
	cargo clean
	rm -f $(OUTPUT_DIR32)/$(BINARY)
	rm -f $(OUTPUT_DIR32E)/$(BINARY)
	rm -f $(OUTPUT_DIR64)/$(BINARY)
	rm -f $(OUTPUT_DIR64E)/$(BINARY)

clean-all:
	cargo clean

help:
	@echo "Usage:"
	@echo "  make        - Build all targets"
	@echo "  make rv32i  - Build RV32I (32 regs)"
	@echo "  make rv32e  - Build RV32E (16 regs)"
	@echo "  make rv64i  - Build RV64I (32 regs)"
	@echo "  make rv64e  - Build RV64E (16 regs)"
	@echo "  make host   - Build for host (native baseline)"
	@echo "  make clean  - Clean build artifacts"
	@echo ""
	@echo "Output binaries go to:"
	@echo "  bin/reth/rv32i/reth-validator"
	@echo "  bin/reth/rv32e/reth-validator"
	@echo "  bin/reth/rv64i/reth-validator"
	@echo "  bin/reth/rv64e/reth-validator"

.PHONY: all rv32i rv32e rv64i rv64e host clean clean-all help
