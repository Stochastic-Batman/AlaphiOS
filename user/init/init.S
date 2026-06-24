.globl _start
_start:
    /* Query current brk */
    xor %edi, %edi
    mov $17, %eax           /* SYS_BRK */
    syscall                 /* rax = current brk */

    /* Grow heap by one page */
    lea 0x1000(%rax), %rdi
    mov $17, %eax           /* SYS_BRK */
    syscall                 /* rax = new brk */

    /* Write to the new heap page (demand-paged) */
    sub $0x1000, %rax
    movl $0xCAFEBABE, (%rax)

    /* getpid then exit with pid */
    mov $3, %eax            /* SYS_GETPID */
    syscall
    mov %eax, %edi
    mov $1, %eax            /* SYS_EXIT */
    syscall
    jmp .
