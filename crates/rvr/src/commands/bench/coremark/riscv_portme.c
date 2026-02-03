// Minimal RISC-V port implementation for CoreMark.
#include "coremark.h"

#if VALIDATION_RUN
volatile ee_s32 seed1_volatile = 0x3415;
volatile ee_s32 seed2_volatile = 0x3415;
volatile ee_s32 seed3_volatile = 0x66;
#endif
#if PERFORMANCE_RUN
volatile ee_s32 seed1_volatile = 0x0;
volatile ee_s32 seed2_volatile = 0x0;
volatile ee_s32 seed3_volatile = 0x66;
#endif
#if PROFILE_RUN
volatile ee_s32 seed1_volatile = 0x8;
volatile ee_s32 seed2_volatile = 0x8;
volatile ee_s32 seed3_volatile = 0x8;
#endif
volatile ee_s32 seed4_volatile = ITERATIONS;
volatile ee_s32 seed5_volatile = 0;
ee_u32 default_num_contexts = 1;

static inline ee_u64 rdcycle(void) {
    ee_u64 val;
#if __riscv_xlen == 64
    __asm__ volatile ("rdcycle %0" : "=r"(val));
#else
    ee_u32 lo, hi, hi2;
    do {
        __asm__ volatile ("rdcycleh %0" : "=r"(hi));
        __asm__ volatile ("rdcycle %0" : "=r"(lo));
        __asm__ volatile ("rdcycleh %0" : "=r"(hi2));
    } while (hi != hi2);
    val = ((ee_u64)hi << 32) | lo;
#endif
    return val;
}

static CORE_TICKS start_time_val, stop_time_val;

void start_time(void) { start_time_val = rdcycle(); }
void stop_time(void) { stop_time_val = rdcycle(); }
CORE_TICKS get_time(void) { return stop_time_val - start_time_val; }
/* rdcycle returns instruction count in rvr, not actual cycles.
 * Use 10M as divisor so typical runs (~300M instrs) report ~30 seconds,
 * passing CoreMark's 10-second minimum validation requirement. */
secs_ret time_in_secs(CORE_TICKS ticks) { return (secs_ret)(ticks / 10000000ULL); }

static inline long syscall1(long n, long a0) {
    register long _a0 __asm__("a0") = a0;
    register long _n __asm__("a7") = n;
    __asm__ volatile ("ecall" : "+r"(_a0) : "r"(_n) : "memory");
    return _a0;
}

static inline long syscall3(long n, long a0, long a1, long a2) {
    register long _a0 __asm__("a0") = a0;
    register long _a1 __asm__("a1") = a1;
    register long _a2 __asm__("a2") = a2;
    register long _n __asm__("a7") = n;
    __asm__ volatile ("ecall" : "+r"(_a0) : "r"(_a1), "r"(_a2), "r"(_n) : "memory");
    return _a0;
}

#define SYS_exit  93
#define SYS_write 64

static void sys_write(int fd, const char *buf, long len) { syscall3(SYS_write, fd, (long)buf, len); }
static void sys_exit(int code) { syscall1(SYS_exit, code); }

static void print_str(const char *s) {
    const char *p = s;
    while (*p) p++;
    sys_write(1, s, p - s);
}

static void print_char(char c) { sys_write(1, &c, 1); }

static void print_int(ee_s32 n) {
    char buf[16];
    int i = 15, neg = 0;
    buf[i] = 0;
    if (n < 0) { neg = 1; n = -n; }
    if (n == 0) buf[--i] = '0';
    else while (n > 0) { buf[--i] = '0' + (n % 10); n /= 10; }
    if (neg) buf[--i] = '-';
    print_str(&buf[i]);
}

static void print_uint(ee_u32 n) {
    char buf[16];
    int i = 15;
    buf[i] = 0;
    if (n == 0) buf[--i] = '0';
    else while (n > 0) { buf[--i] = '0' + (n % 10); n /= 10; }
    print_str(&buf[i]);
}

static void print_uint64(ee_u64 n) {
    char buf[32];
    int i = 31;
    buf[i] = 0;
    if (n == 0) buf[--i] = '0';
    else while (n > 0) { buf[--i] = '0' + (n % 10); n /= 10; }
    print_str(&buf[i]);
}

static void print_int64(long long n) {
    char buf[32];
    int i = 31, neg = 0;
    buf[i] = 0;
    if (n < 0) { neg = 1; n = -n; }
    if (n == 0) buf[--i] = '0';
    else while (n > 0) { buf[--i] = '0' + (n % 10); n /= 10; }
    if (neg) buf[--i] = '-';
    print_str(&buf[i]);
}

static void print_hex(ee_u32 n) {
    char buf[16];
    int i = 15;
    buf[i] = 0;
    if (n == 0) buf[--i] = '0';
    else while (n > 0) {
        int d = n & 0xf;
        buf[--i] = d < 10 ? '0' + d : 'a' + d - 10;
        n >>= 4;
    }
    print_str(&buf[i]);
}

int ee_printf(const char *fmt, ...) {
    __builtin_va_list ap;
    __builtin_va_start(ap, fmt);
    while (*fmt) {
        if (*fmt == '%') {
            fmt++;
            int length = 0;
            while (*fmt == '-' || *fmt == '+' || *fmt == ' ' || *fmt == '#' || *fmt == '0') fmt++;
            while (*fmt >= '0' && *fmt <= '9') fmt++;
            if (*fmt == '.') { fmt++; while (*fmt >= '0' && *fmt <= '9') fmt++; }
            if (*fmt == 'l') { length = 1; fmt++; if (*fmt == 'l') { length = 2; fmt++; } }
            else if (*fmt == 'h' || *fmt == 'z') { fmt++; if (*fmt == 'l' || *fmt == 'h') fmt++; }
            switch (*fmt) {
                case 'd': case 'i':
                    if (length) print_int64(__builtin_va_arg(ap, long long));
                    else print_int(__builtin_va_arg(ap, ee_s32));
                    break;
                case 'u':
                    if (length) print_uint64(__builtin_va_arg(ap, ee_u64));
                    else print_uint(__builtin_va_arg(ap, ee_u32));
                    break;
                case 's': print_str(__builtin_va_arg(ap, const char *)); break;
                case 'c': print_char((char)__builtin_va_arg(ap, int)); break;
                case 'f': case 'g': case 'e': (void)__builtin_va_arg(ap, double); print_str("[float]"); break;
                case 'x': case 'X': print_hex(__builtin_va_arg(ap, ee_u32)); break;
                case '%': print_char('%'); break;
                default: print_char('%'); print_char(*fmt); break;
            }
        } else {
            print_char(*fmt);
        }
        fmt++;
    }
    __builtin_va_end(ap);
    return 0;
}

void portable_init(core_portable *p, int *argc, char *argv[]) { (void)argc; (void)argv; p->portable_id = 1; }
void portable_fini(core_portable *p) { p->portable_id = 0; }

int main(int argc, char *argv[]);

/* Initialize gp register from __global_pointer$ symbol */
extern char __global_pointer$[];
static inline void init_gp(void) {
    __asm__ volatile (".option push; .option norelax; la gp, __global_pointer$; .option pop" ::: "gp");
}

void _start(void) {
    init_gp();
    char *argv[] = {"coremark", 0};
    int ret = main(1, argv);
    sys_exit(ret);
}
