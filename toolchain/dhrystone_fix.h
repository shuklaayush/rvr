// Fix dhrystone for modern host compilers
// Include this BEFORE dhrystone.h with -include

// Use TIME code path which is simpler and avoids the times() multiple definition issue
#define TIME 1
