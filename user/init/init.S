.globl _start
_start:
    /* Test brk: query, grow, write to heap */
    xor %edi, %edi
    mov $17, %eax           /* SYS_BRK */
    syscall
    lea 0x1000(%rax), %rdi
    mov $17, %eax           /* SYS_BRK */
    syscall
    sub $0x1000, %rax
    movl $0xCAFEBABE, (%rax)

    /* Register signal handler for signal 10 */
    mov $10, %edi
    lea sig_handler(%rip), %rsi
    mov $55, %eax           /* SYS_SIGNAL */
    syscall

    /* Get our pid and save in callee-saved r12 */
    mov $3, %eax            /* SYS_GETPID */
    syscall
    mov %eax, %r12d

    /* Send signal 10 to self */
    mov %r12d, %edi
    mov $10, %esi
    mov $53, %eax           /* SYS_KILL */
    syscall
    /* Signal handler runs via deliver_pending_signal. */
    /* After handler + SYS_SIGRETURN, we resume here. */

    /* Exit with pid */
    mov %r12d, %edi
    mov $1, %eax            /* SYS_EXIT */
    syscall
    jmp .

sig_handler:
    ret
