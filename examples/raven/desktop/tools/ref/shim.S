/* The reference machine's reset code.
 *
 * QEMU's Versatile PB starts the processor at address zero and runs whatever is
 * there. This board's firmware is at the bottom of its flash and answers at zero
 * through the static memory controller's remap bit, which QEMU does not model --
 * so the reference run cannot use `images/bootrom.bin` there.
 *
 * This is the minimum that puts QEMU's machine into the state the boot ROM
 * leaves it in: the three images already placed at the addresses
 * `boards/versatile-pb.mjs` names, and the registers the kernel's entry
 * protocol asks for. It is not part of the emulator and never runs in the
 * project -- `tools/ref.sh` is the only thing that uses it, and its whole
 * purpose is to make QEMU a reference for *this* board's memory image rather
 * than for Linux in general.
 *
 * r0 is zero by the protocol, r1 is the machine number a device-tree boot
 * ignores, and r2 is the physical address of the device tree -- which is the
 * one register the kernel actually reads. */

    .section .text.start, "ax"
    .arm
    .global _start

    .equ KERNEL_DEST, 0x00008000
    .equ MACH_VERSATILE_PB, 0x00000183
    .equ DTB_DEST, 0x00c00000

_start:
    mov     r0, #0
    ldr     r1, =MACH_VERSATILE_PB
    ldr     r2, =DTB_DEST
    ldr     pc, =KERNEL_DEST
