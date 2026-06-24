.globl _start
_start:
    /* Test login: SYS_LOGIN(uid=1, password_ptr, password_len) */
    mov $1, %edi
    lea password(%rip), %rsi
    mov $4, %edx
    mov $48, %eax           /* SYS_LOGIN */
    syscall

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

    /* Exit with pid */
    mov %r12d, %edi
    mov $1, %eax            /* SYS_EXIT */
    syscall
    jmp .

sig_handler:
    ret

password:
    .ascii "test"
