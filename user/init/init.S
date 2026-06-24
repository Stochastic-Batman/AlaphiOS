.globl _start
_start:
    mov $3, %eax        /* SYS_GETPID */
    syscall
    mov %eax, %edi      /* exit code = pid */
    mov $1, %eax        /* SYS_EXIT */
    syscall
    jmp .
