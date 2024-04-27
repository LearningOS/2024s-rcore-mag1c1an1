# Report 1

正确进入 U 态后，程序的特征还应有：使用 S 态特权指令，访问 S 态寄存器后会报错。 请同学们可以自行测试这些内容（运行 三个 bad 测例 (ch2b_bad_*.rs) ）， 描述程序出错行为，同时注意注明你使用的 sbi 及其版本。
>rust sbi 0.3.1
>
>1. invalid address  
[kernel] PageFault in application, bad addr = 0x0, bad instruction = 0x804003ac, kernel killed it.
>2. invalid sret  
[kernel] IllegalInstruction in application, kernel killed it.
>3. invalid read csr  
[kernel] IllegalInstruction in application, kernel killed it.  

深入理解 trap.S 中两个函数 __alltraps 和__restore 的作用，并回答如下问题:

 L40：刚进入 __restore 时，a0 代表了什么值。请指出__restore 的两种使用情景。
 > a0 为内核栈的栈顶
 > user app 第一次从内核态到用户态
 > 处理中断/异常

L43-L48：这几行汇编代码特殊处理了哪些寄存器？这些寄存器的的值对于进入用户态有何意义？请分别解释。  

```asm
ld t0, 32*8(sp)
ld t1, 33*8(sp)
ld t2, 2*8(sp)
csrw sstatus, t0
csrw sepc, t1
csrw sscratch, t2
```

> 根据 (repr(c)) trapcontext 和 app_init_cx 知, t0 是 sstatus, t1 是 sepc, t2 是 user sp 之前都存在内核栈上  
 sepc 指示了之后 pc 的值  
 用户态调用函数需要 user sp  
 sstatus.spp 需要在 trap 前后保持一致  

L50-L56：为何跳过了 x2 和 x4？

```asm
ld x1, 1*8(sp)
ld x3, 3*8(sp)
.set n, 5
.rept 27
   LOAD_GP %n
   .set n, n+1
.endr
```

> x2 是 sp, 现在指向内核栈不能改, x4是tp 指向 thraed local 现在没有意义

L60：该指令之后，sp 和 sscratch 中的值分别有什么意义？

```asm
csrrw sp, sscratch, sp
```

> sp 用户栈, sscratch内核栈

__restore：中发生状态切换在哪一条指令？为何该指令执行之后会进入用户态？
> sret, 此时 sstatus.spp 是 u

L13：该指令之后，sp 和 sscratch 中的值分别有什么意义？

```asm
csrrw sp, sscratch, sp
```

> sp 内核栈, sscratch用户栈

从 U 态进入 S 态是哪一条指令发生的？

> ecall

2024.4.24:  
改进:
我觉得 lab 没有必要用 rustsbi, rcore 用这个是为了移植, 而 lab 其实不需要, 直接用 opensbi 和 sbi-rt 感觉就行了, 这样能较少萌新的痛苦.

谢谢 rcore 社区

## Honor Code

在完成本次实验的过程（含此前学习的过程）中，我曾分别与 以下各位 就（与本次实验相关的）以下方面做过交流，还在代码中对应的位置以注释形式记录了具体的交流对象及内容：

None

此外，我也参考了以下资料 ，还在代码中对应的位置以注释形式记录了具体的参考来源及内容：

None

我独立完成了本次实验除以上方面之外的所有工作，包括代码与文档。 我清楚地知道，从以上方面获得的信息在一定程度上降低了实验难度，可能会影响起评分。

我从未使用过他人的代码，不管是原封不动地复制，还是经过了某些等价转换。 我未曾也不会向他人（含此后各届同学）复制或公开我的实验代码，我有义务妥善保管好它们。 我提交至本实验的评测系统的代码，均无意于破坏或妨碍任何计算机系统的正常运转。 我清楚地知道，以上情况均为本课程纪律所禁止，若违反，对应的实验成绩将按“-100”分计。
