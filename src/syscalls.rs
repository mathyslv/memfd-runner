use syscaller::wrap_syscall;

wrap_syscall! {
    1 : ssize_t write(int fd, void *buf, size_t count),
    3 : int close(int fd),
    57 : int fork(),
    59 : int execve(const char *path, char *const *argv, char *const *envp),
    61 : int wait4(int pid, int *status, int options, void *rusage),
    319 : int memfd_create(const char *name, unsigned int flags),
}
