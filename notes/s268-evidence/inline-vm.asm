000000000008b450 <_Wfl.fl_vm_exec$a5081fe92876eb37>:
   8b450:	55                   	push   %rbp
   8b451:	41 57                	push   %r15
   8b453:	41 56                	push   %r14
   8b455:	41 55                	push   %r13
   8b457:	41 54                	push   %r12
   8b459:	53                   	push   %rbx
   8b45a:	48 81 ec 48 20 00 00 	sub    $0x2048,%rsp
   8b461:	48 89 94 24 88 00 00 	mov    %rdx,0x88(%rsp)
   8b468:	00 
   8b469:	48 8b 87 b4 00 00 00 	mov    0xb4(%rdi),%rax
   8b470:	48 39 70 08          	cmp    %rsi,0x8(%rax)
   8b474:	0f 8e 3b 5e 00 00    	jle    912b5 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5e65>
   8b47a:	48 89 f5             	mov    %rsi,%rbp
   8b47d:	49 89 fe             	mov    %rdi,%r14
   8b480:	0f b6 87 ae 04 00 00 	movzbl 0x4ae(%rdi),%eax
   8b487:	88 44 24 37          	mov    %al,0x37(%rsp)
   8b48b:	48 89 f0             	mov    %rsi,%rax
   8b48e:	48 c1 e0 06          	shl    $0x6,%rax
   8b492:	48 89 84 24 18 01 00 	mov    %rax,0x118(%rsp)
   8b499:	00 
   8b49a:	48 c7 84 24 a8 00 00 	movq   $0xffffffffffffffff,0xa8(%rsp)
   8b4a1:	00 ff ff ff ff 
   8b4a6:	48 c7 44 24 68 00 00 	movq   $0x0,0x68(%rsp)
   8b4ad:	00 00 
   8b4af:	48 c7 44 24 78 00 00 	movq   $0x0,0x78(%rsp)
   8b4b6:	00 00 
   8b4b8:	45 31 e4             	xor    %r12d,%r12d
   8b4bb:	48 c7 84 24 b0 00 00 	movq   $0xffffffffffffffff,0xb0(%rsp)
   8b4c2:	00 ff ff ff ff 
   8b4c7:	48 89 74 24 08       	mov    %rsi,0x8(%rsp)
   8b4cc:	eb 23                	jmp    8b4f1 <_Wfl.fl_vm_exec$a5081fe92876eb37+0xa1>
   8b4ce:	4c 89 f7             	mov    %r14,%rdi
   8b4d1:	e8 8a c0 02 00       	call   b7560 <_Wfl.fl_push_overflow$ec8cb5daf6580c79>
   8b4d6:	48 8b 6c 24 08       	mov    0x8(%rsp),%rbp
   8b4db:	4c 8b 24 24          	mov    (%rsp),%r12
   8b4df:	90                   	nop
   8b4e0:	49 8b 86 b4 00 00 00 	mov    0xb4(%r14),%rax
   8b4e7:	48 39 68 08          	cmp    %rbp,0x8(%rax)
   8b4eb:	0f 8e c4 5d 00 00    	jle    912b5 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5e65>
   8b4f1:	41 80 7e 50 00       	cmpb   $0x0,0x50(%r14)
   8b4f6:	74 08                	je     8b500 <_Wfl.fl_vm_exec$a5081fe92876eb37+0xb0>
   8b4f8:	4c 89 f7             	mov    %r14,%rdi
   8b4fb:	e8 90 9d ff ff       	call   85290 <_Wfl.fl_gc_collect$edc7491238b27429>
   8b500:	49 8b 86 c5 01 00 00 	mov    0x1c5(%r14),%rax
   8b507:	48 85 c0             	test   %rax,%rax
   8b50a:	74 45                	je     8b551 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x101>
   8b50c:	49 8b 8e bd 01 00 00 	mov    0x1bd(%r14),%rcx
   8b513:	48 ff c1             	inc    %rcx
   8b516:	0f 80 1a 5e 00 00    	jo     91336 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5ee6>
   8b51c:	49 89 8e bd 01 00 00 	mov    %rcx,0x1bd(%r14)
   8b523:	48 39 c1             	cmp    %rax,%rcx
   8b526:	7e 29                	jle    8b551 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x101>
   8b528:	ba 05 00 00 00       	mov    $0x5,%edx
   8b52d:	41 b8 13 00 00 00    	mov    $0x13,%r8d
   8b533:	4c 89 f7             	mov    %r14,%rdi
   8b536:	48 8d 35 bf 92 f8 ff 	lea    -0x76d41(%rip),%rsi        # 147fc <__abi_tag+0x14500>
   8b53d:	48 8d 0d bd 92 f8 ff 	lea    -0x76d43(%rip),%rcx        # 14801 <__abi_tag+0x14505>
   8b544:	4c 8d 8c 24 f0 1f 00 	lea    0x1ff0(%rsp),%r9
   8b54b:	00 
   8b54c:	e8 7f 75 08 00       	call   112ad0 <_Wfl.fl_raise_bytes.bytesview.4$e8b5fa06fc8adc2d>
   8b551:	41 80 be 5c 01 00 00 	cmpb   $0x0,0x15c(%r14)
   8b558:	00 
   8b559:	74 55                	je     8b5b0 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x160>
   8b55b:	41 80 be 5c 01 00 00 	cmpb   $0x0,0x15c(%r14)
   8b562:	00 
   8b563:	0f 84 77 ff ff ff    	je     8b4e0 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x90>
   8b569:	49 8b 96 b4 00 00 00 	mov    0xb4(%r14),%rdx
   8b570:	49 8b 86 bc 00 00 00 	mov    0xbc(%r14),%rax
   8b577:	48 8b 48 08          	mov    0x8(%rax),%rcx
   8b57b:	48 3b 6a 08          	cmp    0x8(%rdx),%rbp
   8b57f:	0f 8d fb 09 00 00    	jge    8bf80 <_Wfl.fl_vm_exec$a5081fe92876eb37+0xb30>
   8b585:	0f 83 ae 61 00 00    	jae    91739 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x62e9>
   8b58b:	48 8b 12             	mov    (%rdx),%rdx
   8b58e:	48 8b b4 24 18 01 00 	mov    0x118(%rsp),%rsi
   8b595:	00 
   8b596:	48 8b 54 32 38       	mov    0x38(%rdx,%rsi,1),%rdx
   8b59b:	48 85 c9             	test   %rcx,%rcx
   8b59e:	0f 85 e7 09 00 00    	jne    8bf8b <_Wfl.fl_vm_exec$a5081fe92876eb37+0xb3b>
   8b5a4:	e9 24 5d 00 00       	jmp    912cd <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5e7d>
   8b5a9:	0f 1f 80 00 00 00 00 	nopl   0x0(%rax)
   8b5b0:	49 8b 86 b4 00 00 00 	mov    0xb4(%r14),%rax
   8b5b7:	48 8b 48 08          	mov    0x8(%rax),%rcx
   8b5bb:	49 89 cd             	mov    %rcx,%r13
   8b5be:	49 ff cd             	dec    %r13
   8b5c1:	0f 80 be 5d 00 00    	jo     91385 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5f35>
   8b5c7:	49 8b 1e             	mov    (%r14),%rbx
   8b5ca:	49 8b 56 52          	mov    0x52(%r14),%rdx
   8b5ce:	4c 3b ac 24 b0 00 00 	cmp    0xb0(%rsp),%r13
   8b5d5:	00 
   8b5d6:	75 0e                	jne    8b5e6 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x196>
   8b5d8:	48 3b 94 24 a8 00 00 	cmp    0xa8(%rsp),%rdx
   8b5df:	00 
   8b5e0:	0f 84 da 00 00 00    	je     8b6c0 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x270>
   8b5e6:	49 39 cd             	cmp    %rcx,%r13
   8b5e9:	0f 83 b8 61 00 00    	jae    917a7 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6357>
   8b5ef:	48 8b 30             	mov    (%rax),%rsi
   8b5f2:	4c 89 ef             	mov    %r13,%rdi
   8b5f5:	48 c1 e7 06          	shl    $0x6,%rdi
   8b5f9:	4c 8b 14 3e          	mov    (%rsi,%rdi,1),%r10
   8b5fd:	4d 8d 42 f4          	lea    -0xc(%r10),%r8
   8b601:	49 83 f8 f8          	cmp    $0xfffffffffffffff8,%r8
   8b605:	0f 82 26 60 00 00    	jb     91631 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x61e1>
   8b60b:	4c 8b 5c 3e 08       	mov    0x8(%rsi,%rdi,1),%r11
   8b610:	4d 85 db             	test   %r11,%r11
   8b613:	0f 88 18 60 00 00    	js     91631 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x61e1>
   8b619:	45 89 d8             	mov    %r11d,%r8d
   8b61c:	4c 8b 4b 08          	mov    0x8(%rbx),%r9
   8b620:	4c 89 44 24 78       	mov    %r8,0x78(%rsp)
   8b625:	4d 39 c8             	cmp    %r9,%r8
   8b628:	0f 8d 03 60 00 00    	jge    91631 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x61e1>
   8b62e:	4c 8b 03             	mov    (%rbx),%r8
   8b631:	4c 69 7c 24 78 60 01 	imul   $0x160,0x78(%rsp),%r15
   8b638:	00 00 
   8b63a:	4f 39 14 38          	cmp    %r10,(%r8,%r15,1)
   8b63e:	0f 85 ed 5f 00 00    	jne    91631 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x61e1>
   8b644:	49 c1 eb 20          	shr    $0x20,%r11
   8b648:	4f 39 5c 38 19       	cmp    %r11,0x19(%r8,%r15,1)
   8b64d:	0f 85 de 5f 00 00    	jne    91631 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x61e1>
   8b653:	4f 8b 94 38 c9 00 00 	mov    0xc9(%r8,%r15,1),%r10
   8b65a:	00 
   8b65b:	4d 8d 5a f4          	lea    -0xc(%r10),%r11
   8b65f:	49 83 fb f8          	cmp    $0xfffffffffffffff8,%r11
   8b663:	0f 82 b5 5f 00 00    	jb     9161e <_Wfl.fl_vm_exec$a5081fe92876eb37+0x61ce>
   8b669:	4f 8b 9c 38 d1 00 00 	mov    0xd1(%r8,%r15,1),%r11
   8b670:	00 
   8b671:	4d 85 db             	test   %r11,%r11
   8b674:	0f 88 a4 5f 00 00    	js     9161e <_Wfl.fl_vm_exec$a5081fe92876eb37+0x61ce>
   8b67a:	45 89 dc             	mov    %r11d,%r12d
   8b67d:	4d 39 cc             	cmp    %r9,%r12
   8b680:	0f 83 98 5f 00 00    	jae    9161e <_Wfl.fl_vm_exec$a5081fe92876eb37+0x61ce>
   8b686:	4d 69 cc 60 01 00 00 	imul   $0x160,%r12,%r9
   8b68d:	4f 39 14 08          	cmp    %r10,(%r8,%r9,1)
   8b691:	0f 85 87 5f 00 00    	jne    9161e <_Wfl.fl_vm_exec$a5081fe92876eb37+0x61ce>
   8b697:	49 c1 eb 20          	shr    $0x20,%r11
   8b69b:	4f 39 5c 08 19       	cmp    %r11,0x19(%r8,%r9,1)
   8b6a0:	0f 85 78 5f 00 00    	jne    9161e <_Wfl.fl_vm_exec$a5081fe92876eb37+0x61ce>
   8b6a6:	48 8b 74 3e 20       	mov    0x20(%rsi,%rdi,1),%rsi
   8b6ab:	48 89 74 24 68       	mov    %rsi,0x68(%rsp)
   8b6b0:	4c 89 ac 24 b0 00 00 	mov    %r13,0xb0(%rsp)
   8b6b7:	00 
   8b6b8:	48 89 94 24 a8 00 00 	mov    %rdx,0xa8(%rsp)
   8b6bf:	00 
   8b6c0:	49 39 cd             	cmp    %rcx,%r13
   8b6c3:	0f 83 0d 60 00 00    	jae    916d6 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6286>
   8b6c9:	4c 89 ef             	mov    %r13,%rdi
   8b6cc:	48 c1 e7 06          	shl    $0x6,%rdi
   8b6d0:	48 8b 10             	mov    (%rax),%rdx
   8b6d3:	4c 8b 7c 3a 18       	mov    0x18(%rdx,%rdi,1),%r15
   8b6d8:	80 7c 24 37 00       	cmpb   $0x0,0x37(%rsp)
   8b6dd:	48 89 7c 24 18       	mov    %rdi,0x18(%rsp)
   8b6e2:	74 62                	je     8b746 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x2f6>
   8b6e4:	4d 85 ff             	test   %r15,%r15
   8b6e7:	78 1f                	js     8b708 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x2b8>
   8b6e9:	4c 3b 63 08          	cmp    0x8(%rbx),%r12
   8b6ed:	0f 83 22 61 00 00    	jae    91815 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x63c5>
   8b6f3:	48 8b 13             	mov    (%rbx),%rdx
   8b6f6:	49 69 f4 60 01 00 00 	imul   $0x160,%r12,%rsi
   8b6fd:	48 8b 54 32 61       	mov    0x61(%rdx,%rsi,1),%rdx
   8b702:	4c 3b 7a 08          	cmp    0x8(%rdx),%r15
   8b706:	7c 3e                	jl     8b746 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x2f6>
   8b708:	ba 0c 00 00 00       	mov    $0xc,%edx
   8b70d:	4c 89 f7             	mov    %r14,%rdi
   8b710:	48 8d 35 29 91 f8 ff 	lea    -0x76ed7(%rip),%rsi        # 14840 <__abi_tag+0x14544>
   8b717:	4c 89 e1             	mov    %r12,%rcx
   8b71a:	4d 89 f8             	mov    %r15,%r8
   8b71d:	45 31 c9             	xor    %r9d,%r9d
   8b720:	6a 2c                	push   $0x2c
   8b722:	48 8d 05 eb 90 f8 ff 	lea    -0x76f15(%rip),%rax        # 14814 <__abi_tag+0x14518>
   8b729:	50                   	push   %rax
   8b72a:	e8 31 15 07 00       	call   fcc60 <_Wfl.fl_vm_bug_at$494622488d4f7894>
   8b72f:	48 8b 7c 24 28       	mov    0x28(%rsp),%rdi
   8b734:	48 83 c4 10          	add    $0x10,%rsp
   8b738:	49 8b 1e             	mov    (%r14),%rbx
   8b73b:	49 8b 86 b4 00 00 00 	mov    0xb4(%r14),%rax
   8b742:	48 8b 48 08          	mov    0x8(%rax),%rcx
   8b746:	49 39 cd             	cmp    %rcx,%r13
   8b749:	0f 83 66 5f 00 00    	jae    916b5 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6265>
   8b74f:	48 8b 10             	mov    (%rax),%rdx
   8b752:	48 8b 4c 3a 18       	mov    0x18(%rdx,%rdi,1),%rcx
   8b757:	48 89 ce             	mov    %rcx,%rsi
   8b75a:	48 ff c6             	inc    %rsi
   8b75d:	0f 80 43 5c 00 00    	jo     913a6 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5f56>
   8b763:	48 89 74 3a 18       	mov    %rsi,0x18(%rdx,%rdi,1)
   8b768:	4c 3b 63 08          	cmp    0x8(%rbx),%r12
   8b76c:	0f 83 e0 5e 00 00    	jae    91652 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6202>
   8b772:	48 8b 13             	mov    (%rbx),%rdx
   8b775:	49 69 f4 60 01 00 00 	imul   $0x160,%r12,%rsi
   8b77c:	48 89 74 24 10       	mov    %rsi,0x10(%rsp)
   8b781:	48 8b 54 32 61       	mov    0x61(%rdx,%rsi,1),%rdx
   8b786:	48 3b 4a 08          	cmp    0x8(%rdx),%rcx
   8b78a:	0f 83 c2 5e 00 00    	jae    91652 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6202>
   8b790:	48 8b 12             	mov    (%rdx),%rdx
   8b793:	44 0f b6 1c 0a       	movzbl (%rdx,%rcx,1),%r11d
   8b798:	41 0f b6 cb          	movzbl %r11b,%ecx
   8b79c:	48 89 4c 24 50       	mov    %rcx,0x50(%rsp)
   8b7a1:	80 7c 24 37 00       	cmpb   $0x0,0x37(%rsp)
   8b7a6:	4c 89 5c 24 28       	mov    %r11,0x28(%rsp)
   8b7ab:	0f 84 6a 0a 00 00    	je     8c21b <_Wfl.fl_vm_exec$a5081fe92876eb37+0xdcb>
   8b7b1:	41 80 fb 3c          	cmp    $0x3c,%r11b
   8b7b5:	72 37                	jb     8b7ee <_Wfl.fl_vm_exec$a5081fe92876eb37+0x39e>
   8b7b7:	ba 0c 00 00 00       	mov    $0xc,%edx
   8b7bc:	4c 89 f7             	mov    %r14,%rdi
   8b7bf:	48 8d 35 7a 90 f8 ff 	lea    -0x76f86(%rip),%rsi        # 14840 <__abi_tag+0x14544>
   8b7c6:	4c 89 e1             	mov    %r12,%rcx
   8b7c9:	4d 89 f8             	mov    %r15,%r8
   8b7cc:	4c 8b 4c 24 50       	mov    0x50(%rsp),%r9
   8b7d1:	6a 15                	push   $0x15
   8b7d3:	48 8d 05 72 90 f8 ff 	lea    -0x76f8e(%rip),%rax        # 1484c <__abi_tag+0x14550>
   8b7da:	50                   	push   %rax
   8b7db:	e8 80 14 07 00       	call   fcc60 <_Wfl.fl_vm_bug_at$494622488d4f7894>
   8b7e0:	48 83 c4 10          	add    $0x10,%rsp
   8b7e4:	49 8b 1e             	mov    (%r14),%rbx
   8b7e7:	49 8b 86 b4 00 00 00 	mov    0xb4(%r14),%rax
   8b7ee:	49 8b 4e 48          	mov    0x48(%r14),%rcx
   8b7f2:	48 89 9c 24 78 19 00 	mov    %rbx,0x1978(%rsp)
   8b7f9:	00 
   8b7fa:	41 0f 10 46 08       	movups 0x8(%r14),%xmm0
   8b7ff:	0f 11 84 24 80 19 00 	movups %xmm0,0x1980(%rsp)
   8b806:	00 
   8b807:	41 0f 10 46 18       	movups 0x18(%r14),%xmm0
   8b80c:	0f 11 84 24 90 19 00 	movups %xmm0,0x1990(%rsp)
   8b813:	00 
   8b814:	41 0f 10 46 28       	movups 0x28(%r14),%xmm0
   8b819:	0f 11 84 24 a0 19 00 	movups %xmm0,0x19a0(%rsp)
   8b820:	00 
   8b821:	41 0f 10 46 38       	movups 0x38(%r14),%xmm0
   8b826:	0f 11 84 24 b0 19 00 	movups %xmm0,0x19b0(%rsp)
   8b82d:	00 
   8b82e:	48 89 8c 24 c0 19 00 	mov    %rcx,0x19c0(%rsp)
   8b835:	00 
   8b836:	41 0f b7 4e 50       	movzwl 0x50(%r14),%ecx
   8b83b:	48 89 8c 24 c8 19 00 	mov    %rcx,0x19c8(%rsp)
   8b842:	00 
   8b843:	49 8b 4e 62          	mov    0x62(%r14),%rcx
   8b847:	41 0f 10 46 52       	movups 0x52(%r14),%xmm0
   8b84c:	0f 11 84 24 d0 19 00 	movups %xmm0,0x19d0(%rsp)
   8b853:	00 
   8b854:	48 89 8c 24 e0 19 00 	mov    %rcx,0x19e0(%rsp)
   8b85b:	00 
   8b85c:	49 8b 8e 8a 00 00 00 	mov    0x8a(%r14),%rcx
   8b863:	41 0f 10 46 6a       	movups 0x6a(%r14),%xmm0
   8b868:	0f 11 84 24 e8 19 00 	movups %xmm0,0x19e8(%rsp)
   8b86f:	00 
   8b870:	41 0f 10 46 7a       	movups 0x7a(%r14),%xmm0
   8b875:	0f 11 84 24 f8 19 00 	movups %xmm0,0x19f8(%rsp)
   8b87c:	00 
   8b87d:	48 89 8c 24 08 1a 00 	mov    %rcx,0x1a08(%rsp)
   8b884:	00 
   8b885:	41 0f b7 8e a2 00 00 	movzwl 0xa2(%r14),%ecx
   8b88c:	00 
   8b88d:	41 0f 10 86 92 00 00 	movups 0x92(%r14),%xmm0
   8b894:	00 
   8b895:	0f 11 84 24 10 1a 00 	movups %xmm0,0x1a10(%rsp)
   8b89c:	00 
   8b89d:	41 0f 10 86 a4 00 00 	movups 0xa4(%r14),%xmm0
   8b8a4:	00 
   8b8a5:	48 89 8c 24 20 1a 00 	mov    %rcx,0x1a20(%rsp)
   8b8ac:	00 
   8b8ad:	41 0f b6 8e 5c 01 00 	movzbl 0x15c(%r14),%ecx
   8b8b4:	00 
   8b8b5:	0f 11 84 24 28 1a 00 	movups %xmm0,0x1a28(%rsp)
   8b8bc:	00 
   8b8bd:	48 89 84 24 38 1a 00 	mov    %rax,0x1a38(%rsp)
   8b8c4:	00 
   8b8c5:	41 0f 10 86 bc 00 00 	movups 0xbc(%r14),%xmm0
   8b8cc:	00 
   8b8cd:	0f 11 84 24 40 1a 00 	movups %xmm0,0x1a40(%rsp)
   8b8d4:	00 
   8b8d5:	41 0f 10 86 cc 00 00 	movups 0xcc(%r14),%xmm0
   8b8dc:	00 
   8b8dd:	0f 11 84 24 50 1a 00 	movups %xmm0,0x1a50(%rsp)
   8b8e4:	00 
   8b8e5:	41 0f 10 86 dc 00 00 	movups 0xdc(%r14),%xmm0
   8b8ec:	00 
   8b8ed:	0f 11 84 24 60 1a 00 	movups %xmm0,0x1a60(%rsp)
   8b8f4:	00 
   8b8f5:	41 0f 10 86 ec 00 00 	movups 0xec(%r14),%xmm0
   8b8fc:	00 
   8b8fd:	0f 11 84 24 70 1a 00 	movups %xmm0,0x1a70(%rsp)
   8b904:	00 
   8b905:	41 0f 10 86 fc 00 00 	movups 0xfc(%r14),%xmm0
   8b90c:	00 
   8b90d:	0f 11 84 24 80 1a 00 	movups %xmm0,0x1a80(%rsp)
   8b914:	00 
   8b915:	41 0f 10 86 0c 01 00 	movups 0x10c(%r14),%xmm0
   8b91c:	00 
   8b91d:	0f 11 84 24 90 1a 00 	movups %xmm0,0x1a90(%rsp)
   8b924:	00 
   8b925:	41 0f 10 86 1c 01 00 	movups 0x11c(%r14),%xmm0
   8b92c:	00 
   8b92d:	0f 11 84 24 a0 1a 00 	movups %xmm0,0x1aa0(%rsp)
   8b934:	00 
   8b935:	41 0f 10 86 2c 01 00 	movups 0x12c(%r14),%xmm0
   8b93c:	00 
   8b93d:	0f 11 84 24 b0 1a 00 	movups %xmm0,0x1ab0(%rsp)
   8b944:	00 
   8b945:	41 0f 10 86 3c 01 00 	movups 0x13c(%r14),%xmm0
   8b94c:	00 
   8b94d:	0f 11 84 24 c0 1a 00 	movups %xmm0,0x1ac0(%rsp)
   8b954:	00 
   8b955:	41 0f 10 86 4c 01 00 	movups 0x14c(%r14),%xmm0
   8b95c:	00 
   8b95d:	0f 11 84 24 d0 1a 00 	movups %xmm0,0x1ad0(%rsp)
   8b964:	00 
   8b965:	41 0f 10 86 5d 01 00 	movups 0x15d(%r14),%xmm0
   8b96c:	00 
   8b96d:	88 8c 24 e0 1a 00 00 	mov    %cl,0x1ae0(%rsp)
   8b974:	0f 11 84 24 e8 1a 00 	movups %xmm0,0x1ae8(%rsp)
   8b97b:	00 
   8b97c:	41 0f 10 86 6d 01 00 	movups 0x16d(%r14),%xmm0
   8b983:	00 
   8b984:	41 0f b6 86 dd 01 00 	movzbl 0x1dd(%r14),%eax
   8b98b:	00 
   8b98c:	49 8b 8e ee 01 00 00 	mov    0x1ee(%r14),%rcx
   8b993:	0f 11 84 24 f8 1a 00 	movups %xmm0,0x1af8(%rsp)
   8b99a:	00 
   8b99b:	41 0f 10 86 7d 01 00 	movups 0x17d(%r14),%xmm0
   8b9a2:	00 
   8b9a3:	0f 11 84 24 08 1b 00 	movups %xmm0,0x1b08(%rsp)
   8b9aa:	00 
   8b9ab:	41 0f 10 86 8d 01 00 	movups 0x18d(%r14),%xmm0
   8b9b2:	00 
   8b9b3:	0f 11 84 24 18 1b 00 	movups %xmm0,0x1b18(%rsp)
   8b9ba:	00 
   8b9bb:	41 0f 10 86 9d 01 00 	movups 0x19d(%r14),%xmm0
   8b9c2:	00 
   8b9c3:	0f 11 84 24 28 1b 00 	movups %xmm0,0x1b28(%rsp)
   8b9ca:	00 
   8b9cb:	41 0f 10 86 ad 01 00 	movups 0x1ad(%r14),%xmm0
   8b9d2:	00 
   8b9d3:	0f 11 84 24 38 1b 00 	movups %xmm0,0x1b38(%rsp)
   8b9da:	00 
   8b9db:	41 0f 10 86 bd 01 00 	movups 0x1bd(%r14),%xmm0
   8b9e2:	00 
   8b9e3:	0f 11 84 24 48 1b 00 	movups %xmm0,0x1b48(%rsp)
   8b9ea:	00 
   8b9eb:	41 0f 10 86 cd 01 00 	movups 0x1cd(%r14),%xmm0
   8b9f2:	00 
   8b9f3:	0f 11 84 24 58 1b 00 	movups %xmm0,0x1b58(%rsp)
   8b9fa:	00 
   8b9fb:	88 84 24 68 1b 00 00 	mov    %al,0x1b68(%rsp)
   8ba02:	49 8b 86 16 02 00 00 	mov    0x216(%r14),%rax
   8ba09:	41 0f 10 86 de 01 00 	movups 0x1de(%r14),%xmm0
   8ba10:	00 
   8ba11:	0f 11 84 24 70 1b 00 	movups %xmm0,0x1b70(%rsp)
   8ba18:	00 
   8ba19:	48 89 8c 24 80 1b 00 	mov    %rcx,0x1b80(%rsp)
   8ba20:	00 
   8ba21:	41 0f b6 8e 1e 02 00 	movzbl 0x21e(%r14),%ecx
   8ba28:	00 
   8ba29:	41 0f 10 86 f6 01 00 	movups 0x1f6(%r14),%xmm0
   8ba30:	00 
   8ba31:	0f 11 84 24 88 1b 00 	movups %xmm0,0x1b88(%rsp)
   8ba38:	00 
   8ba39:	41 0f 10 86 06 02 00 	movups 0x206(%r14),%xmm0
   8ba40:	00 
   8ba41:	0f 11 84 24 98 1b 00 	movups %xmm0,0x1b98(%rsp)
   8ba48:	00 
   8ba49:	48 89 84 24 a8 1b 00 	mov    %rax,0x1ba8(%rsp)
   8ba50:	00 
   8ba51:	49 8b 86 2f 02 00 00 	mov    0x22f(%r14),%rax
   8ba58:	48 89 8c 24 b0 1b 00 	mov    %rcx,0x1bb0(%rsp)
   8ba5f:	00 
   8ba60:	49 8b 8e 47 02 00 00 	mov    0x247(%r14),%rcx
   8ba67:	41 0f 10 86 1f 02 00 	movups 0x21f(%r14),%xmm0
   8ba6e:	00 
   8ba6f:	0f 11 84 24 b8 1b 00 	movups %xmm0,0x1bb8(%rsp)
   8ba76:	00 
   8ba77:	48 89 84 24 c8 1b 00 	mov    %rax,0x1bc8(%rsp)
   8ba7e:	00 
   8ba7f:	49 8b 86 7f 02 00 00 	mov    0x27f(%r14),%rax
   8ba86:	41 0f 10 86 37 02 00 	movups 0x237(%r14),%xmm0
   8ba8d:	00 
   8ba8e:	0f 11 84 24 d0 1b 00 	movups %xmm0,0x1bd0(%rsp)
   8ba95:	00 
   8ba96:	41 0f 10 86 4f 02 00 	movups 0x24f(%r14),%xmm0
   8ba9d:	00 
   8ba9e:	48 89 8c 24 e0 1b 00 	mov    %rcx,0x1be0(%rsp)
   8baa5:	00 
   8baa6:	0f 11 84 24 e8 1b 00 	movups %xmm0,0x1be8(%rsp)
   8baad:	00 
   8baae:	41 0f 10 86 5f 02 00 	movups 0x25f(%r14),%xmm0
   8bab5:	00 
   8bab6:	0f 11 84 24 f8 1b 00 	movups %xmm0,0x1bf8(%rsp)
   8babd:	00 
   8babe:	41 0f 10 86 6f 02 00 	movups 0x26f(%r14),%xmm0
   8bac5:	00 
   8bac6:	0f 11 84 24 08 1c 00 	movups %xmm0,0x1c08(%rsp)
   8bacd:	00 
   8bace:	41 0f 10 86 87 02 00 	movups 0x287(%r14),%xmm0
   8bad5:	00 
   8bad6:	48 89 84 24 18 1c 00 	mov    %rax,0x1c18(%rsp)
   8badd:	00 
   8bade:	0f 11 84 24 20 1c 00 	movups %xmm0,0x1c20(%rsp)
   8bae5:	00 
   8bae6:	41 0f 10 86 97 02 00 	movups 0x297(%r14),%xmm0
   8baed:	00 
   8baee:	0f 11 84 24 30 1c 00 	movups %xmm0,0x1c30(%rsp)
   8baf5:	00 
   8baf6:	41 0f 10 86 a7 02 00 	movups 0x2a7(%r14),%xmm0
   8bafd:	00 
   8bafe:	0f 11 84 24 40 1c 00 	movups %xmm0,0x1c40(%rsp)
   8bb05:	00 
   8bb06:	41 0f 10 86 b7 02 00 	movups 0x2b7(%r14),%xmm0
   8bb0d:	00 
   8bb0e:	0f 11 84 24 50 1c 00 	movups %xmm0,0x1c50(%rsp)
   8bb15:	00 
   8bb16:	41 0f 10 86 c7 02 00 	movups 0x2c7(%r14),%xmm0
   8bb1d:	00 
   8bb1e:	0f 11 84 24 60 1c 00 	movups %xmm0,0x1c60(%rsp)
   8bb25:	00 
   8bb26:	41 0f 10 86 d7 02 00 	movups 0x2d7(%r14),%xmm0
   8bb2d:	00 
   8bb2e:	0f 11 84 24 70 1c 00 	movups %xmm0,0x1c70(%rsp)
   8bb35:	00 
   8bb36:	41 0f 10 86 e7 02 00 	movups 0x2e7(%r14),%xmm0
   8bb3d:	00 
   8bb3e:	0f 11 84 24 80 1c 00 	movups %xmm0,0x1c80(%rsp)
   8bb45:	00 
   8bb46:	41 0f 10 86 f7 02 00 	movups 0x2f7(%r14),%xmm0
   8bb4d:	00 
   8bb4e:	0f 11 84 24 90 1c 00 	movups %xmm0,0x1c90(%rsp)
   8bb55:	00 
   8bb56:	41 0f 10 86 07 03 00 	movups 0x307(%r14),%xmm0
   8bb5d:	00 
   8bb5e:	49 8b 86 17 03 00 00 	mov    0x317(%r14),%rax
   8bb65:	49 8b 8e 3f 03 00 00 	mov    0x33f(%r14),%rcx
   8bb6c:	0f 11 84 24 a0 1c 00 	movups %xmm0,0x1ca0(%rsp)
   8bb73:	00 
   8bb74:	48 89 84 24 b0 1c 00 	mov    %rax,0x1cb0(%rsp)
   8bb7b:	00 
   8bb7c:	49 8b 86 57 03 00 00 	mov    0x357(%r14),%rax
   8bb83:	41 0f 10 86 1f 03 00 	movups 0x31f(%r14),%xmm0
   8bb8a:	00 
   8bb8b:	0f 11 84 24 b8 1c 00 	movups %xmm0,0x1cb8(%rsp)
   8bb92:	00 
   8bb93:	41 0f 10 86 2f 03 00 	movups 0x32f(%r14),%xmm0
   8bb9a:	00 
   8bb9b:	0f 11 84 24 c8 1c 00 	movups %xmm0,0x1cc8(%rsp)
   8bba2:	00 
   8bba3:	48 89 8c 24 d8 1c 00 	mov    %rcx,0x1cd8(%rsp)
   8bbaa:	00 
   8bbab:	41 0f b6 8e 5f 03 00 	movzbl 0x35f(%r14),%ecx
   8bbb2:	00 
   8bbb3:	41 0f 10 86 47 03 00 	movups 0x347(%r14),%xmm0
   8bbba:	00 
   8bbbb:	0f 11 84 24 e0 1c 00 	movups %xmm0,0x1ce0(%rsp)
   8bbc2:	00 
   8bbc3:	48 89 84 24 f0 1c 00 	mov    %rax,0x1cf0(%rsp)
   8bbca:	00 
   8bbcb:	49 8b 86 90 03 00 00 	mov    0x390(%r14),%rax
   8bbd2:	48 89 8c 24 f8 1c 00 	mov    %rcx,0x1cf8(%rsp)
   8bbd9:	00 
   8bbda:	41 0f b6 8e a8 03 00 	movzbl 0x3a8(%r14),%ecx
   8bbe1:	00 
   8bbe2:	41 0f 10 86 60 03 00 	movups 0x360(%r14),%xmm0
   8bbe9:	00 
   8bbea:	0f 11 84 24 00 1d 00 	movups %xmm0,0x1d00(%rsp)
   8bbf1:	00 
   8bbf2:	41 0f 10 86 70 03 00 	movups 0x370(%r14),%xmm0
   8bbf9:	00 
   8bbfa:	0f 11 84 24 10 1d 00 	movups %xmm0,0x1d10(%rsp)
   8bc01:	00 
   8bc02:	41 0f 10 86 80 03 00 	movups 0x380(%r14),%xmm0
   8bc09:	00 
   8bc0a:	0f 11 84 24 20 1d 00 	movups %xmm0,0x1d20(%rsp)
   8bc11:	00 
   8bc12:	48 89 84 24 30 1d 00 	mov    %rax,0x1d30(%rsp)
   8bc19:	00 
   8bc1a:	49 8b 86 b9 03 00 00 	mov    0x3b9(%r14),%rax
   8bc21:	41 0f 10 86 98 03 00 	movups 0x398(%r14),%xmm0
   8bc28:	00 
   8bc29:	0f 11 84 24 38 1d 00 	movups %xmm0,0x1d38(%rsp)
   8bc30:	00 
   8bc31:	48 89 8c 24 48 1d 00 	mov    %rcx,0x1d48(%rsp)
   8bc38:	00 
   8bc39:	49 8b 8e 01 04 00 00 	mov    0x401(%r14),%rcx
   8bc40:	41 0f 10 86 a9 03 00 	movups 0x3a9(%r14),%xmm0
   8bc47:	00 
   8bc48:	0f 11 84 24 50 1d 00 	movups %xmm0,0x1d50(%rsp)
   8bc4f:	00 
   8bc50:	41 0f 10 86 c1 03 00 	movups 0x3c1(%r14),%xmm0
   8bc57:	00 
   8bc58:	48 89 84 24 60 1d 00 	mov    %rax,0x1d60(%rsp)
   8bc5f:	00 
   8bc60:	0f 11 84 24 68 1d 00 	movups %xmm0,0x1d68(%rsp)
   8bc67:	00 
   8bc68:	41 0f 10 86 d1 03 00 	movups 0x3d1(%r14),%xmm0
   8bc6f:	00 
   8bc70:	0f 11 84 24 78 1d 00 	movups %xmm0,0x1d78(%rsp)
   8bc77:	00 
   8bc78:	41 0f 10 86 e1 03 00 	movups 0x3e1(%r14),%xmm0
   8bc7f:	00 
   8bc80:	0f 11 84 24 88 1d 00 	movups %xmm0,0x1d88(%rsp)
   8bc87:	00 
   8bc88:	41 0f 10 86 f1 03 00 	movups 0x3f1(%r14),%xmm0
   8bc8f:	00 
   8bc90:	0f 11 84 24 98 1d 00 	movups %xmm0,0x1d98(%rsp)
   8bc97:	00 
   8bc98:	41 0f 10 86 09 04 00 	movups 0x409(%r14),%xmm0
   8bc9f:	00 
   8bca0:	48 89 8c 24 a8 1d 00 	mov    %rcx,0x1da8(%rsp)
   8bca7:	00 
   8bca8:	0f 11 84 24 b0 1d 00 	movups %xmm0,0x1db0(%rsp)
   8bcaf:	00 
   8bcb0:	41 0f 10 86 19 04 00 	movups 0x419(%r14),%xmm0
   8bcb7:	00 
   8bcb8:	41 0f b6 86 69 04 00 	movzbl 0x469(%r14),%eax
   8bcbf:	00 
   8bcc0:	41 0f b7 8e 6a 04 00 	movzwl 0x46a(%r14),%ecx
   8bcc7:	00 
   8bcc8:	c1 e1 08             	shl    $0x8,%ecx
   8bccb:	48 09 c1             	or     %rax,%rcx
   8bcce:	49 8b 86 49 04 00 00 	mov    0x449(%r14),%rax
   8bcd5:	0f 11 84 24 c0 1d 00 	movups %xmm0,0x1dc0(%rsp)
   8bcdc:	00 
   8bcdd:	41 0f 10 86 29 04 00 	movups 0x429(%r14),%xmm0
   8bce4:	00 
   8bce5:	0f 11 84 24 d0 1d 00 	movups %xmm0,0x1dd0(%rsp)
   8bcec:	00 
   8bced:	41 0f 10 86 39 04 00 	movups 0x439(%r14),%xmm0
   8bcf4:	00 
   8bcf5:	0f 11 84 24 e0 1d 00 	movups %xmm0,0x1de0(%rsp)
   8bcfc:	00 
   8bcfd:	48 89 84 24 f0 1d 00 	mov    %rax,0x1df0(%rsp)
   8bd04:	00 
   8bd05:	49 8b 86 61 04 00 00 	mov    0x461(%r14),%rax
   8bd0c:	41 0f 10 86 51 04 00 	movups 0x451(%r14),%xmm0
   8bd13:	00 
   8bd14:	0f 11 84 24 f8 1d 00 	movups %xmm0,0x1df8(%rsp)
   8bd1b:	00 
   8bd1c:	48 89 84 24 08 1e 00 	mov    %rax,0x1e08(%rsp)
   8bd23:	00 
   8bd24:	41 0f 10 86 6c 04 00 	movups 0x46c(%r14),%xmm0
   8bd2b:	00 
   8bd2c:	48 89 8c 24 10 1e 00 	mov    %rcx,0x1e10(%rsp)
   8bd33:	00 
   8bd34:	0f 11 84 24 18 1e 00 	movups %xmm0,0x1e18(%rsp)
   8bd3b:	00 
   8bd3c:	41 0f 10 86 7c 04 00 	movups 0x47c(%r14),%xmm0
   8bd43:	00 
   8bd44:	41 0f b7 86 8c 04 00 	movzwl 0x48c(%r14),%eax
   8bd4b:	00 
   8bd4c:	0f 11 84 24 28 1e 00 	movups %xmm0,0x1e28(%rsp)
   8bd53:	00 
   8bd54:	41 0f 10 86 8e 04 00 	movups 0x48e(%r14),%xmm0
   8bd5b:	00 
   8bd5c:	48 89 84 24 38 1e 00 	mov    %rax,0x1e38(%rsp)
   8bd63:	00 
   8bd64:	0f 11 84 24 40 1e 00 	movups %xmm0,0x1e40(%rsp)
   8bd6b:	00 
   8bd6c:	41 0f 10 86 9e 04 00 	movups 0x49e(%r14),%xmm0
   8bd73:	00 
   8bd74:	41 0f b6 86 ae 04 00 	movzbl 0x4ae(%r14),%eax
   8bd7b:	00 
   8bd7c:	0f 11 84 24 50 1e 00 	movups %xmm0,0x1e50(%rsp)
   8bd83:	00 
   8bd84:	41 0f 10 86 af 04 00 	movups 0x4af(%r14),%xmm0
   8bd8b:	00 
   8bd8c:	88 84 24 60 1e 00 00 	mov    %al,0x1e60(%rsp)
   8bd93:	0f 11 84 24 68 1e 00 	movups %xmm0,0x1e68(%rsp)
   8bd9a:	00 
   8bd9b:	41 0f 10 86 bf 04 00 	movups 0x4bf(%r14),%xmm0
   8bda2:	00 
   8bda3:	0f 11 84 24 78 1e 00 	movups %xmm0,0x1e78(%rsp)
   8bdaa:	00 
   8bdab:	41 0f 10 86 cf 04 00 	movups 0x4cf(%r14),%xmm0
   8bdb2:	00 
   8bdb3:	0f 11 84 24 88 1e 00 	movups %xmm0,0x1e88(%rsp)
   8bdba:	00 
   8bdbb:	49 8b 86 df 04 00 00 	mov    0x4df(%r14),%rax
   8bdc2:	48 89 84 24 98 1e 00 	mov    %rax,0x1e98(%rsp)
   8bdc9:	00 
   8bdca:	48 8d bc 24 78 19 00 	lea    0x1978(%rsp),%rdi
   8bdd1:	00 
   8bdd2:	4c 89 e6             	mov    %r12,%rsi
   8bdd5:	4c 89 fa             	mov    %r15,%rdx
   8bdd8:	e8 23 de 02 00       	call   b9c00 <_Wfl.check_op_len$52db95ddc731f12e>
   8bddd:	84 d2                	test   %dl,%dl
   8bddf:	74 26                	je     8be07 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x9b7>
   8bde1:	4c 01 f8             	add    %r15,%rax
   8bde4:	0f 80 f3 55 00 00    	jo     913dd <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5f8d>
   8bdea:	4c 3b 63 08          	cmp    0x8(%rbx),%r12
   8bdee:	0f 83 42 5a 00 00    	jae    91836 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x63e6>
   8bdf4:	48 8b 0b             	mov    (%rbx),%rcx
   8bdf7:	48 8b 54 24 10       	mov    0x10(%rsp),%rdx
   8bdfc:	48 8b 4c 11 61       	mov    0x61(%rcx,%rdx,1),%rcx
   8be01:	48 3b 41 08          	cmp    0x8(%rcx),%rax
   8be05:	7e 2d                	jle    8be34 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x9e4>
   8be07:	ba 0c 00 00 00       	mov    $0xc,%edx
   8be0c:	4c 89 f7             	mov    %r14,%rdi
   8be0f:	48 8d 35 2a 8a f8 ff 	lea    -0x775d6(%rip),%rsi        # 14840 <__abi_tag+0x14544>
   8be16:	4c 89 e1             	mov    %r12,%rcx
   8be19:	4d 89 f8             	mov    %r15,%r8
   8be1c:	4c 8b 4c 24 50       	mov    0x50(%rsp),%r9
   8be21:	6a 1c                	push   $0x1c
   8be23:	48 8d 05 37 8a f8 ff 	lea    -0x775c9(%rip),%rax        # 14861 <__abi_tag+0x14565>
   8be2a:	50                   	push   %rax
   8be2b:	e8 30 0e 07 00       	call   fcc60 <_Wfl.fl_vm_bug_at$494622488d4f7894>
   8be30:	48 83 c4 10          	add    $0x10,%rsp
   8be34:	48 8b 5c 24 50       	mov    0x50(%rsp),%rbx
   8be39:	83 fb 32             	cmp    $0x32,%ebx
   8be3c:	4c 8b 5c 24 28       	mov    0x28(%rsp),%r11
   8be41:	0f 87 af 00 00 00    	ja     8bef6 <_Wfl.fl_vm_exec$a5081fe92876eb37+0xaa6>
   8be47:	89 d8                	mov    %ebx,%eax
   8be49:	48 b9 01 c0 01 00 80 	movabs $0x430800001c001,%rcx
   8be50:	30 04 00 
   8be53:	48 0f a3 c1          	bt     %rax,%rcx
   8be57:	0f 83 99 00 00 00    	jae    8bef6 <_Wfl.fl_vm_exec$a5081fe92876eb37+0xaa6>
   8be5d:	4c 89 f8             	mov    %r15,%rax
   8be60:	48 ff c0             	inc    %rax
   8be63:	0f 80 06 55 00 00    	jo     9136f <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5f1f>
   8be69:	49 8b 0e             	mov    (%r14),%rcx
   8be6c:	4c 3b 61 08          	cmp    0x8(%rcx),%r12
   8be70:	0f 83 fd 57 00 00    	jae    91673 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6223>
   8be76:	48 8b 09             	mov    (%rcx),%rcx
   8be79:	48 8b 54 24 10       	mov    0x10(%rsp),%rdx
   8be7e:	48 8b 54 11 61       	mov    0x61(%rcx,%rdx,1),%rdx
   8be83:	48 8b 7a 08          	mov    0x8(%rdx),%rdi
   8be87:	48 39 f8             	cmp    %rdi,%rax
   8be8a:	0f 83 e3 57 00 00    	jae    91673 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6223>
   8be90:	48 89 c6             	mov    %rax,%rsi
   8be93:	48 ff c6             	inc    %rsi
   8be96:	0f 80 d3 54 00 00    	jo     9136f <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5f1f>
   8be9c:	48 39 fe             	cmp    %rdi,%rsi
   8be9f:	0f 83 ce 57 00 00    	jae    91673 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6223>
   8bea5:	48 8b 12             	mov    (%rdx),%rdx
   8bea8:	0f b6 04 02          	movzbl (%rdx,%rax,1),%eax
   8beac:	0f b6 14 32          	movzbl (%rdx,%rsi,1),%edx
   8beb0:	c1 e2 08             	shl    $0x8,%edx
   8beb3:	48 09 c2             	or     %rax,%rdx
   8beb6:	48 8b 44 24 10       	mov    0x10(%rsp),%rax
   8bebb:	48 8b 44 01 29       	mov    0x29(%rcx,%rax,1),%rax
   8bec0:	48 3b 50 08          	cmp    0x8(%rax),%rdx
   8bec4:	7c 30                	jl     8bef6 <_Wfl.fl_vm_exec$a5081fe92876eb37+0xaa6>
   8bec6:	ba 0c 00 00 00       	mov    $0xc,%edx
   8becb:	4c 89 f7             	mov    %r14,%rdi
   8bece:	48 8d 35 6b 89 f8 ff 	lea    -0x77695(%rip),%rsi        # 14840 <__abi_tag+0x14544>
   8bed5:	4c 89 e1             	mov    %r12,%rcx
   8bed8:	4d 89 f8             	mov    %r15,%r8
   8bedb:	49 89 d9             	mov    %rbx,%r9
   8bede:	6a 22                	push   $0x22
   8bee0:	48 8d 05 96 89 f8 ff 	lea    -0x7766a(%rip),%rax        # 1487d <__abi_tag+0x14581>
   8bee7:	50                   	push   %rax
   8bee8:	e8 73 0d 07 00       	call   fcc60 <_Wfl.fl_vm_bug_at$494622488d4f7894>
   8beed:	4c 8b 5c 24 38       	mov    0x38(%rsp),%r11
   8bef2:	48 83 c4 10          	add    $0x10,%rsp
   8bef6:	41 8d 43 f7          	lea    -0x9(%r11),%eax
   8befa:	3c 01                	cmp    $0x1,%al
   8befc:	0f 87 c1 01 00 00    	ja     8c0c3 <_Wfl.fl_vm_exec$a5081fe92876eb37+0xc73>
   8bf02:	49 8b 0e             	mov    (%r14),%rcx
   8bf05:	4c 3b 61 08          	cmp    0x8(%rcx),%r12
   8bf09:	0f 83 82 58 00 00    	jae    91791 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6341>
   8bf0f:	4c 89 f8             	mov    %r15,%rax
   8bf12:	48 ff c0             	inc    %rax
   8bf15:	0f 80 ac 54 00 00    	jo     913c7 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5f77>
   8bf1b:	48 8b 11             	mov    (%rcx),%rdx
   8bf1e:	48 8b 74 24 10       	mov    0x10(%rsp),%rsi
   8bf23:	48 8b 4c 32 61       	mov    0x61(%rdx,%rsi,1),%rcx
   8bf28:	48 3b 41 08          	cmp    0x8(%rcx),%rax
   8bf2c:	0f 83 5f 58 00 00    	jae    91791 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6341>
   8bf32:	48 8b 94 32 99 00 00 	mov    0x99(%rdx,%rsi,1),%rdx
   8bf39:	00 
   8bf3a:	48 ff c2             	inc    %rdx
   8bf3d:	0f 80 84 54 00 00    	jo     913c7 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5f77>
   8bf43:	48 8b 09             	mov    (%rcx),%rcx
   8bf46:	0f b6 04 01          	movzbl (%rcx,%rax,1),%eax
   8bf4a:	48 39 c2             	cmp    %rax,%rdx
   8bf4d:	0f 8f 06 02 00 00    	jg     8c159 <_Wfl.fl_vm_exec$a5081fe92876eb37+0xd09>
   8bf53:	ba 0c 00 00 00       	mov    $0xc,%edx
   8bf58:	4c 89 f7             	mov    %r14,%rdi
   8bf5b:	48 8d 35 de 88 f8 ff 	lea    -0x77722(%rip),%rsi        # 14840 <__abi_tag+0x14544>
   8bf62:	4c 89 e1             	mov    %r12,%rcx
   8bf65:	4d 89 f8             	mov    %r15,%r8
   8bf68:	49 89 d9             	mov    %rbx,%r9
   8bf6b:	6a 23                	push   $0x23
   8bf6d:	48 8d 05 2b 89 f8 ff 	lea    -0x776d5(%rip),%rax        # 1489f <__abi_tag+0x145a3>
   8bf74:	e9 d1 01 00 00       	jmp    8c14a <_Wfl.fl_vm_exec$a5081fe92876eb37+0xcfa>
   8bf79:	0f 1f 80 00 00 00 00 	nopl   0x0(%rax)
   8bf80:	31 d2                	xor    %edx,%edx
   8bf82:	48 85 c9             	test   %rcx,%rcx
   8bf85:	0f 84 42 53 00 00    	je     912cd <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5e7d>
   8bf8b:	48 89 ce             	mov    %rcx,%rsi
   8bf8e:	48 ff ce             	dec    %rsi
   8bf91:	0f 80 b2 53 00 00    	jo     91349 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5ef9>
   8bf97:	48 39 ce             	cmp    %rcx,%rsi
   8bf9a:	0f 83 e9 56 00 00    	jae    91689 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6239>
   8bfa0:	48 8b 38             	mov    (%rax),%rdi
   8bfa3:	49 89 f0             	mov    %rsi,%r8
   8bfa6:	49 c1 e0 05          	shl    $0x5,%r8
   8bfaa:	4a 39 6c 07 10       	cmp    %rbp,0x10(%rdi,%r8,1)
   8bfaf:	0f 8e 29 53 00 00    	jle    912de <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5e8e>
   8bfb5:	4d 89 e5             	mov    %r12,%r13
   8bfb8:	4a 8b 54 07 18       	mov    0x18(%rdi,%r8,1),%rdx
   8bfbd:	49 89 96 b5 01 00 00 	mov    %rdx,0x1b5(%r14)
   8bfc4:	48 85 c9             	test   %rcx,%rcx
   8bfc7:	7e 17                	jle    8bfe0 <_Wfl.fl_vm_exec$a5081fe92876eb37+0xb90>
   8bfc9:	48 89 70 08          	mov    %rsi,0x8(%rax)
   8bfcd:	4e 8b 24 07          	mov    (%rdi,%r8,1),%r12
   8bfd1:	4a 8b 5c 07 08       	mov    0x8(%rdi,%r8,1),%rbx
   8bfd6:	4e 8b 7c 07 10       	mov    0x10(%rdi,%r8,1),%r15
   8bfdb:	eb 0b                	jmp    8bfe8 <_Wfl.fl_vm_exec$a5081fe92876eb37+0xb98>
   8bfdd:	0f 1f 00             	nopl   (%rax)
   8bfe0:	45 31 ff             	xor    %r15d,%r15d
   8bfe3:	31 db                	xor    %ebx,%ebx
   8bfe5:	45 31 e4             	xor    %r12d,%r12d
   8bfe8:	4c 89 f7             	mov    %r14,%rdi
   8bfeb:	48 89 de             	mov    %rbx,%rsi
   8bfee:	e8 ad b6 02 00       	call   b76a0 <_Wfl.fl_close_upvals$345768e8f0248c70>
   8bff3:	49 8b 86 b4 00 00 00 	mov    0xb4(%r14),%rax
   8bffa:	48 8b 48 08          	mov    0x8(%rax),%rcx
   8bffe:	66 90                	xchg   %ax,%ax
   8c000:	4c 39 f9             	cmp    %r15,%rcx
   8c003:	7e 1b                	jle    8c020 <_Wfl.fl_vm_exec$a5081fe92876eb37+0xbd0>
   8c005:	48 85 c9             	test   %rcx,%rcx
   8c008:	7e f6                	jle    8c000 <_Wfl.fl_vm_exec$a5081fe92876eb37+0xbb0>
   8c00a:	48 ff c9             	dec    %rcx
   8c00d:	48 89 48 08          	mov    %rcx,0x8(%rax)
   8c011:	eb ed                	jmp    8c000 <_Wfl.fl_vm_exec$a5081fe92876eb37+0xbb0>
   8c013:	66 66 66 66 2e 0f 1f 	data16 data16 data16 cs nopw 0x0(%rax,%rax,1)
   8c01a:	84 00 00 00 00 00 
   8c020:	49 89 9e ac 00 00 00 	mov    %rbx,0xac(%r14)
   8c027:	41 0f 10 86 5d 01 00 	movups 0x15d(%r14),%xmm0
   8c02e:	00 
   8c02f:	f2 41 0f 10 8e 6d 01 	movsd  0x16d(%r14),%xmm1
   8c036:	00 00 
   8c038:	49 8d 8e 5c 01 00 00 	lea    0x15c(%r14),%rcx
   8c03f:	0f 57 d2             	xorps  %xmm2,%xmm2
   8c042:	0f 11 51 09          	movups %xmm2,0x9(%rcx)
   8c046:	0f 11 11             	movups %xmm2,(%rcx)
   8c049:	48 81 fb 00 40 00 00 	cmp    $0x4000,%rbx
   8c050:	7c 1e                	jl     8c070 <_Wfl.fl_vm_exec$a5081fe92876eb37+0xc20>
   8c052:	4c 89 f7             	mov    %r14,%rdi
   8c055:	e8 06 b5 02 00       	call   b7560 <_Wfl.fl_push_overflow$ec8cb5daf6580c79>
   8c05a:	49 8b 86 b4 00 00 00 	mov    0xb4(%r14),%rax
   8c061:	eb 39                	jmp    8c09c <_Wfl.fl_vm_exec$a5081fe92876eb37+0xc4c>
   8c063:	66 66 66 66 2e 0f 1f 	data16 data16 data16 cs nopw 0x0(%rax,%rax,1)
   8c06a:	84 00 00 00 00 00 
   8c070:	49 8b 8e a4 00 00 00 	mov    0xa4(%r14),%rcx
   8c077:	48 3b 59 08          	cmp    0x8(%rcx),%rbx
   8c07b:	0f 83 d9 56 00 00    	jae    9175a <_Wfl.fl_vm_exec$a5081fe92876eb37+0x630a>
   8c081:	48 8b 09             	mov    (%rcx),%rcx
   8c084:	48 8d 14 5b          	lea    (%rbx,%rbx,2),%rdx
   8c088:	0f 11 04 d1          	movups %xmm0,(%rcx,%rdx,8)
   8c08c:	f2 0f 11 4c d1 10    	movsd  %xmm1,0x10(%rcx,%rdx,8)
   8c092:	48 ff c3             	inc    %rbx
   8c095:	49 89 9e ac 00 00 00 	mov    %rbx,0xac(%r14)
   8c09c:	49 ff cf             	dec    %r15
   8c09f:	0f 80 b7 52 00 00    	jo     9135c <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5f0c>
   8c0a5:	4c 3b 78 08          	cmp    0x8(%rax),%r15
   8c0a9:	0f 83 f0 55 00 00    	jae    9169f <_Wfl.fl_vm_exec$a5081fe92876eb37+0x624f>
   8c0af:	48 8b 00             	mov    (%rax),%rax
   8c0b2:	49 c1 e7 06          	shl    $0x6,%r15
   8c0b6:	4e 89 64 38 18       	mov    %r12,0x18(%rax,%r15,1)
   8c0bb:	4d 89 ec             	mov    %r13,%r12
   8c0be:	e9 1d f4 ff ff       	jmp    8b4e0 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x90>
   8c0c3:	41 8d 43 f5          	lea    -0xb(%r11),%eax
   8c0c7:	3c 01                	cmp    $0x1,%al
   8c0c9:	0f 87 8a 00 00 00    	ja     8c159 <_Wfl.fl_vm_exec$a5081fe92876eb37+0xd09>
   8c0cf:	49 8b 16             	mov    (%r14),%rdx
   8c0d2:	48 8b 42 08          	mov    0x8(%rdx),%rax
   8c0d6:	49 39 c4             	cmp    %rax,%r12
   8c0d9:	0f 83 f4 56 00 00    	jae    917d3 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6383>
   8c0df:	4c 89 f9             	mov    %r15,%rcx
   8c0e2:	48 ff c1             	inc    %rcx
   8c0e5:	0f 80 13 53 00 00    	jo     913fe <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5fae>
   8c0eb:	48 8b 12             	mov    (%rdx),%rdx
   8c0ee:	48 8b 74 24 10       	mov    0x10(%rsp),%rsi
   8c0f3:	48 8b 74 32 61       	mov    0x61(%rdx,%rsi,1),%rsi
   8c0f8:	48 3b 4e 08          	cmp    0x8(%rsi),%rcx
   8c0fc:	0f 83 d1 56 00 00    	jae    917d3 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6383>
   8c102:	48 8b 7c 24 78       	mov    0x78(%rsp),%rdi
   8c107:	48 39 c7             	cmp    %rax,%rdi
   8c10a:	0f 83 c3 56 00 00    	jae    917d3 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6383>
   8c110:	48 8b 06             	mov    (%rsi),%rax
   8c113:	0f b6 04 08          	movzbl (%rax,%rcx,1),%eax
   8c117:	48 69 cf 60 01 00 00 	imul   $0x160,%rdi,%rcx
   8c11e:	48 8b 4c 0a 29       	mov    0x29(%rdx,%rcx,1),%rcx
   8c123:	48 39 41 08          	cmp    %rax,0x8(%rcx)
   8c127:	7f 30                	jg     8c159 <_Wfl.fl_vm_exec$a5081fe92876eb37+0xd09>
   8c129:	ba 0c 00 00 00       	mov    $0xc,%edx
   8c12e:	4c 89 f7             	mov    %r14,%rdi
   8c131:	48 8d 35 08 87 f8 ff 	lea    -0x778f8(%rip),%rsi        # 14840 <__abi_tag+0x14544>
   8c138:	4c 89 e1             	mov    %r12,%rcx
   8c13b:	4d 89 f8             	mov    %r15,%r8
   8c13e:	49 89 d9             	mov    %rbx,%r9
   8c141:	6a 21                	push   $0x21
   8c143:	48 8d 05 78 87 f8 ff 	lea    -0x77888(%rip),%rax        # 148c2 <__abi_tag+0x145c6>
   8c14a:	50                   	push   %rax
   8c14b:	e8 10 0b 07 00       	call   fcc60 <_Wfl.fl_vm_bug_at$494622488d4f7894>
   8c150:	4c 8b 5c 24 38       	mov    0x38(%rsp),%r11
   8c155:	48 83 c4 10          	add    $0x10,%rsp
   8c159:	49 8b 86 ac 00 00 00 	mov    0xac(%r14),%rax
   8c160:	48 3b 44 24 68       	cmp    0x68(%rsp),%rax
   8c165:	7d 37                	jge    8c19e <_Wfl.fl_vm_exec$a5081fe92876eb37+0xd4e>
   8c167:	ba 0c 00 00 00       	mov    $0xc,%edx
   8c16c:	4c 89 f7             	mov    %r14,%rdi
   8c16f:	48 8d 35 ca 86 f8 ff 	lea    -0x77936(%rip),%rsi        # 14840 <__abi_tag+0x14544>
   8c176:	4c 89 e1             	mov    %r12,%rcx
   8c179:	4d 89 f8             	mov    %r15,%r8
   8c17c:	49 89 d9             	mov    %rbx,%r9
   8c17f:	6a 16                	push   $0x16
   8c181:	48 8d 05 5b 87 f8 ff 	lea    -0x778a5(%rip),%rax        # 148e3 <__abi_tag+0x145e7>
   8c188:	50                   	push   %rax
   8c189:	e8 d2 0a 07 00       	call   fcc60 <_Wfl.fl_vm_bug_at$494622488d4f7894>
   8c18e:	4c 8b 5c 24 38       	mov    0x38(%rsp),%r11
   8c193:	48 83 c4 10          	add    $0x10,%rsp
   8c197:	49 8b 86 ac 00 00 00 	mov    0xac(%r14),%rax
   8c19e:	49 8b 8e a4 00 00 00 	mov    0xa4(%r14),%rcx
   8c1a5:	48 3b 41 08          	cmp    0x8(%rcx),%rax
   8c1a9:	7e 30                	jle    8c1db <_Wfl.fl_vm_exec$a5081fe92876eb37+0xd8b>
   8c1ab:	ba 0c 00 00 00       	mov    $0xc,%edx
   8c1b0:	4c 89 f7             	mov    %r14,%rdi
   8c1b3:	48 8d 35 86 86 f8 ff 	lea    -0x7797a(%rip),%rsi        # 14840 <__abi_tag+0x14544>
   8c1ba:	4c 89 e1             	mov    %r12,%rcx
   8c1bd:	4d 89 f8             	mov    %r15,%r8
   8c1c0:	49 89 d9             	mov    %rbx,%r9
   8c1c3:	6a 21                	push   $0x21
   8c1c5:	48 8d 05 2d 87 f8 ff 	lea    -0x778d3(%rip),%rax        # 148f9 <__abi_tag+0x145fd>
   8c1cc:	50                   	push   %rax
   8c1cd:	e8 8e 0a 07 00       	call   fcc60 <_Wfl.fl_vm_bug_at$494622488d4f7894>
   8c1d2:	4c 8b 5c 24 38       	mov    0x38(%rsp),%r11
   8c1d7:	48 83 c4 10          	add    $0x10,%rsp
   8c1db:	49 8b 86 bc 00 00 00 	mov    0xbc(%r14),%rax
   8c1e2:	48 83 78 08 41       	cmpq   $0x41,0x8(%rax)
   8c1e7:	7c 32                	jl     8c21b <_Wfl.fl_vm_exec$a5081fe92876eb37+0xdcb>
   8c1e9:	ba 0c 00 00 00       	mov    $0xc,%edx
   8c1ee:	4c 89 f7             	mov    %r14,%rdi
   8c1f1:	48 8d 35 48 86 f8 ff 	lea    -0x779b8(%rip),%rsi        # 14840 <__abi_tag+0x14544>
   8c1f8:	4c 89 e1             	mov    %r12,%rcx
   8c1fb:	4d 89 f8             	mov    %r15,%r8
   8c1fe:	4c 8b 4c 24 50       	mov    0x50(%rsp),%r9
   8c203:	6a 1d                	push   $0x1d
   8c205:	48 8d 05 0e 87 f8 ff 	lea    -0x778f2(%rip),%rax        # 1491a <__abi_tag+0x1461e>
   8c20c:	50                   	push   %rax
   8c20d:	e8 4e 0a 07 00       	call   fcc60 <_Wfl.fl_vm_bug_at$494622488d4f7894>
   8c212:	4c 8b 5c 24 38       	mov    0x38(%rsp),%r11
   8c217:	48 83 c4 10          	add    $0x10,%rsp
   8c21b:	4c 89 7c 24 70       	mov    %r15,0x70(%rsp)
   8c220:	4c 89 24 24          	mov    %r12,(%rsp)
   8c224:	4d 8b 4e 20          	mov    0x20(%r14),%r9
   8c228:	49 8b 7e 48          	mov    0x48(%r14),%rdi
   8c22c:	45 0f b7 46 50       	movzwl 0x50(%r14),%r8d
   8c231:	49 8b 76 62          	mov    0x62(%r14),%rsi
   8c235:	49 8b 96 8a 00 00 00 	mov    0x8a(%r14),%rdx
   8c23c:	41 0f b7 8e a2 00 00 	movzwl 0xa2(%r14),%ecx
   8c243:	00 
   8c244:	49 8b 9e a4 00 00 00 	mov    0xa4(%r14),%rbx
   8c24b:	4d 8b 96 ac 00 00 00 	mov    0xac(%r14),%r10
   8c252:	4d 8b be b4 00 00 00 	mov    0xb4(%r14),%r15
   8c259:	41 0f b6 86 5c 01 00 	movzbl 0x15c(%r14),%eax
   8c260:	00 
   8c261:	41 0f 10 06          	movups (%r14),%xmm0
   8c265:	0f 29 84 24 c0 04 00 	movaps %xmm0,0x4c0(%rsp)
   8c26c:	00 
   8c26d:	41 0f 10 46 10       	movups 0x10(%r14),%xmm0
   8c272:	0f 29 84 24 d0 04 00 	movaps %xmm0,0x4d0(%rsp)
   8c279:	00 
   8c27a:	41 0f 10 46 28       	movups 0x28(%r14),%xmm0
   8c27f:	4c 89 8c 24 e0 04 00 	mov    %r9,0x4e0(%rsp)
   8c286:	00 
   8c287:	0f 11 84 24 e8 04 00 	movups %xmm0,0x4e8(%rsp)
   8c28e:	00 
   8c28f:	41 0f 10 46 38       	movups 0x38(%r14),%xmm0
   8c294:	0f 11 84 24 f8 04 00 	movups %xmm0,0x4f8(%rsp)
   8c29b:	00 
   8c29c:	41 0f 10 46 52       	movups 0x52(%r14),%xmm0
   8c2a1:	48 89 bc 24 08 05 00 	mov    %rdi,0x508(%rsp)
   8c2a8:	00 
   8c2a9:	4c 89 84 24 10 05 00 	mov    %r8,0x510(%rsp)
   8c2b0:	00 
   8c2b1:	0f 11 84 24 18 05 00 	movups %xmm0,0x518(%rsp)
   8c2b8:	00 
   8c2b9:	48 89 b4 24 28 05 00 	mov    %rsi,0x528(%rsp)
   8c2c0:	00 
   8c2c1:	41 0f 10 46 6a       	movups 0x6a(%r14),%xmm0
   8c2c6:	0f 29 84 24 30 05 00 	movaps %xmm0,0x530(%rsp)
   8c2cd:	00 
   8c2ce:	41 0f 10 46 7a       	movups 0x7a(%r14),%xmm0
   8c2d3:	0f 29 84 24 40 05 00 	movaps %xmm0,0x540(%rsp)
   8c2da:	00 
   8c2db:	41 0f 10 86 92 00 00 	movups 0x92(%r14),%xmm0
   8c2e2:	00 
   8c2e3:	48 89 94 24 50 05 00 	mov    %rdx,0x550(%rsp)
   8c2ea:	00 
   8c2eb:	0f 11 84 24 58 05 00 	movups %xmm0,0x558(%rsp)
   8c2f2:	00 
   8c2f3:	48 89 8c 24 68 05 00 	mov    %rcx,0x568(%rsp)
   8c2fa:	00 
   8c2fb:	48 89 5c 24 38       	mov    %rbx,0x38(%rsp)
   8c300:	48 89 9c 24 70 05 00 	mov    %rbx,0x570(%rsp)
   8c307:	00 
   8c308:	4c 89 54 24 20       	mov    %r10,0x20(%rsp)
   8c30d:	4c 89 94 24 78 05 00 	mov    %r10,0x578(%rsp)
   8c314:	00 
   8c315:	4c 89 bc 24 80 05 00 	mov    %r15,0x580(%rsp)
   8c31c:	00 
   8c31d:	41 0f 10 86 bc 00 00 	movups 0xbc(%r14),%xmm0
   8c324:	00 
   8c325:	0f 11 84 24 88 05 00 	movups %xmm0,0x588(%rsp)
   8c32c:	00 
   8c32d:	41 0f 10 86 cc 00 00 	movups 0xcc(%r14),%xmm0
   8c334:	00 
   8c335:	0f 11 84 24 98 05 00 	movups %xmm0,0x598(%rsp)
   8c33c:	00 
   8c33d:	41 0f 10 86 dc 00 00 	movups 0xdc(%r14),%xmm0
   8c344:	00 
   8c345:	0f 11 84 24 a8 05 00 	movups %xmm0,0x5a8(%rsp)
   8c34c:	00 
   8c34d:	41 0f 10 86 ec 00 00 	movups 0xec(%r14),%xmm0
   8c354:	00 
   8c355:	0f 11 84 24 b8 05 00 	movups %xmm0,0x5b8(%rsp)
   8c35c:	00 
   8c35d:	41 0f 10 86 fc 00 00 	movups 0xfc(%r14),%xmm0
   8c364:	00 
   8c365:	0f 11 84 24 c8 05 00 	movups %xmm0,0x5c8(%rsp)
   8c36c:	00 
   8c36d:	41 0f 10 86 0c 01 00 	movups 0x10c(%r14),%xmm0
   8c374:	00 
   8c375:	0f 11 84 24 d8 05 00 	movups %xmm0,0x5d8(%rsp)
   8c37c:	00 
   8c37d:	41 0f 10 86 1c 01 00 	movups 0x11c(%r14),%xmm0
   8c384:	00 
   8c385:	0f 11 84 24 e8 05 00 	movups %xmm0,0x5e8(%rsp)
   8c38c:	00 
   8c38d:	41 0f 10 86 2c 01 00 	movups 0x12c(%r14),%xmm0
   8c394:	00 
   8c395:	0f 11 84 24 f8 05 00 	movups %xmm0,0x5f8(%rsp)
   8c39c:	00 
   8c39d:	41 0f 10 86 3c 01 00 	movups 0x13c(%r14),%xmm0
   8c3a4:	00 
   8c3a5:	0f 11 84 24 08 06 00 	movups %xmm0,0x608(%rsp)
   8c3ac:	00 
   8c3ad:	41 0f 10 86 4c 01 00 	movups 0x14c(%r14),%xmm0
   8c3b4:	00 
   8c3b5:	0f 11 84 24 18 06 00 	movups %xmm0,0x618(%rsp)
   8c3bc:	00 
   8c3bd:	41 0f 10 86 5d 01 00 	movups 0x15d(%r14),%xmm0
   8c3c4:	00 
   8c3c5:	88 84 24 28 06 00 00 	mov    %al,0x628(%rsp)
   8c3cc:	0f 29 84 24 30 06 00 	movaps %xmm0,0x630(%rsp)
   8c3d3:	00 
   8c3d4:	41 0f 10 86 6d 01 00 	movups 0x16d(%r14),%xmm0
   8c3db:	00 
   8c3dc:	4d 8b a6 cd 01 00 00 	mov    0x1cd(%r14),%r12
   8c3e3:	49 8b 9e d5 01 00 00 	mov    0x1d5(%r14),%rbx
   8c3ea:	41 0f b6 86 dd 01 00 	movzbl 0x1dd(%r14),%eax
   8c3f1:	00 
   8c3f2:	4d 8b 86 ee 01 00 00 	mov    0x1ee(%r14),%r8
   8c3f9:	4d 8b 8e 16 02 00 00 	mov    0x216(%r14),%r9
   8c400:	41 0f b6 be 1e 02 00 	movzbl 0x21e(%r14),%edi
   8c407:	00 
   8c408:	49 8b b6 2f 02 00 00 	mov    0x22f(%r14),%rsi
   8c40f:	49 8b 96 47 02 00 00 	mov    0x247(%r14),%rdx
   8c416:	49 8b 8e 7f 02 00 00 	mov    0x27f(%r14),%rcx
   8c41d:	0f 29 84 24 40 06 00 	movaps %xmm0,0x640(%rsp)
   8c424:	00 
   8c425:	41 0f 10 86 7d 01 00 	movups 0x17d(%r14),%xmm0
   8c42c:	00 
   8c42d:	0f 29 84 24 50 06 00 	movaps %xmm0,0x650(%rsp)
   8c434:	00 
   8c435:	41 0f 10 86 8d 01 00 	movups 0x18d(%r14),%xmm0
   8c43c:	00 
   8c43d:	0f 29 84 24 60 06 00 	movaps %xmm0,0x660(%rsp)
   8c444:	00 
   8c445:	41 0f 10 86 9d 01 00 	movups 0x19d(%r14),%xmm0
   8c44c:	00 
   8c44d:	0f 29 84 24 70 06 00 	movaps %xmm0,0x670(%rsp)
   8c454:	00 
   8c455:	41 0f 10 86 ad 01 00 	movups 0x1ad(%r14),%xmm0
   8c45c:	00 
   8c45d:	0f 29 84 24 80 06 00 	movaps %xmm0,0x680(%rsp)
   8c464:	00 
   8c465:	41 0f 10 86 bd 01 00 	movups 0x1bd(%r14),%xmm0
   8c46c:	00 
   8c46d:	0f 29 84 24 90 06 00 	movaps %xmm0,0x690(%rsp)
   8c474:	00 
   8c475:	41 0f 10 86 de 01 00 	movups 0x1de(%r14),%xmm0
   8c47c:	00 
   8c47d:	4c 89 a4 24 a0 06 00 	mov    %r12,0x6a0(%rsp)
   8c484:	00 
   8c485:	48 89 9c 24 a8 06 00 	mov    %rbx,0x6a8(%rsp)
   8c48c:	00 
   8c48d:	88 84 24 b0 06 00 00 	mov    %al,0x6b0(%rsp)
   8c494:	0f 11 84 24 b8 06 00 	movups %xmm0,0x6b8(%rsp)
   8c49b:	00 
   8c49c:	41 0f 10 86 f6 01 00 	movups 0x1f6(%r14),%xmm0
   8c4a3:	00 
   8c4a4:	4c 89 84 24 c8 06 00 	mov    %r8,0x6c8(%rsp)
   8c4ab:	00 
   8c4ac:	0f 29 84 24 d0 06 00 	movaps %xmm0,0x6d0(%rsp)
   8c4b3:	00 
   8c4b4:	41 0f 10 86 06 02 00 	movups 0x206(%r14),%xmm0
   8c4bb:	00 
   8c4bc:	0f 29 84 24 e0 06 00 	movaps %xmm0,0x6e0(%rsp)
   8c4c3:	00 
   8c4c4:	41 0f 10 86 1f 02 00 	movups 0x21f(%r14),%xmm0
   8c4cb:	00 
   8c4cc:	4c 89 8c 24 f0 06 00 	mov    %r9,0x6f0(%rsp)
   8c4d3:	00 
   8c4d4:	48 89 bc 24 f8 06 00 	mov    %rdi,0x6f8(%rsp)
   8c4db:	00 
   8c4dc:	0f 29 84 24 00 07 00 	movaps %xmm0,0x700(%rsp)
   8c4e3:	00 
   8c4e4:	41 0f 10 86 37 02 00 	movups 0x237(%r14),%xmm0
   8c4eb:	00 
   8c4ec:	48 89 b4 24 10 07 00 	mov    %rsi,0x710(%rsp)
   8c4f3:	00 
   8c4f4:	0f 11 84 24 18 07 00 	movups %xmm0,0x718(%rsp)
   8c4fb:	00 
   8c4fc:	41 0f 10 86 4f 02 00 	movups 0x24f(%r14),%xmm0
   8c503:	00 
   8c504:	48 89 94 24 28 07 00 	mov    %rdx,0x728(%rsp)
   8c50b:	00 
   8c50c:	0f 29 84 24 30 07 00 	movaps %xmm0,0x730(%rsp)
   8c513:	00 
   8c514:	41 0f 10 86 5f 02 00 	movups 0x25f(%r14),%xmm0
   8c51b:	00 
   8c51c:	0f 29 84 24 40 07 00 	movaps %xmm0,0x740(%rsp)
   8c523:	00 
   8c524:	41 0f 10 86 6f 02 00 	movups 0x26f(%r14),%xmm0
   8c52b:	00 
   8c52c:	0f 29 84 24 50 07 00 	movaps %xmm0,0x750(%rsp)
   8c533:	00 
   8c534:	41 0f 10 86 87 02 00 	movups 0x287(%r14),%xmm0
   8c53b:	00 
   8c53c:	48 89 8c 24 60 07 00 	mov    %rcx,0x760(%rsp)
   8c543:	00 
   8c544:	0f 11 84 24 68 07 00 	movups %xmm0,0x768(%rsp)
   8c54b:	00 
   8c54c:	41 0f 10 86 97 02 00 	movups 0x297(%r14),%xmm0
   8c553:	00 
   8c554:	0f 11 84 24 78 07 00 	movups %xmm0,0x778(%rsp)
   8c55b:	00 
   8c55c:	41 0f 10 86 a7 02 00 	movups 0x2a7(%r14),%xmm0
   8c563:	00 
   8c564:	0f 11 84 24 88 07 00 	movups %xmm0,0x788(%rsp)
   8c56b:	00 
   8c56c:	41 0f 10 86 b7 02 00 	movups 0x2b7(%r14),%xmm0
   8c573:	00 
   8c574:	0f 11 84 24 98 07 00 	movups %xmm0,0x798(%rsp)
   8c57b:	00 
   8c57c:	41 0f 10 86 c7 02 00 	movups 0x2c7(%r14),%xmm0
   8c583:	00 
   8c584:	0f 11 84 24 a8 07 00 	movups %xmm0,0x7a8(%rsp)
   8c58b:	00 
   8c58c:	41 0f 10 86 d7 02 00 	movups 0x2d7(%r14),%xmm0
   8c593:	00 
   8c594:	0f 11 84 24 b8 07 00 	movups %xmm0,0x7b8(%rsp)
   8c59b:	00 
   8c59c:	41 0f 10 86 e7 02 00 	movups 0x2e7(%r14),%xmm0
   8c5a3:	00 
   8c5a4:	0f 11 84 24 c8 07 00 	movups %xmm0,0x7c8(%rsp)
   8c5ab:	00 
   8c5ac:	41 0f 10 86 f7 02 00 	movups 0x2f7(%r14),%xmm0
   8c5b3:	00 
   8c5b4:	0f 11 84 24 d8 07 00 	movups %xmm0,0x7d8(%rsp)
   8c5bb:	00 
   8c5bc:	41 0f 10 86 07 03 00 	movups 0x307(%r14),%xmm0
   8c5c3:	00 
   8c5c4:	49 8b 8e 17 03 00 00 	mov    0x317(%r14),%rcx
   8c5cb:	49 8b 96 3f 03 00 00 	mov    0x33f(%r14),%rdx
   8c5d2:	49 8b b6 57 03 00 00 	mov    0x357(%r14),%rsi
   8c5d9:	41 0f b6 be 5f 03 00 	movzbl 0x35f(%r14),%edi
   8c5e0:	00 
   8c5e1:	4d 8b 86 90 03 00 00 	mov    0x390(%r14),%r8
   8c5e8:	45 0f b6 8e a8 03 00 	movzbl 0x3a8(%r14),%r9d
   8c5ef:	00 
   8c5f0:	4d 8b 96 b9 03 00 00 	mov    0x3b9(%r14),%r10
   8c5f7:	0f 11 84 24 e8 07 00 	movups %xmm0,0x7e8(%rsp)
   8c5fe:	00 
   8c5ff:	48 89 8c 24 f8 07 00 	mov    %rcx,0x7f8(%rsp)
   8c606:	00 
   8c607:	49 8b 8e 01 04 00 00 	mov    0x401(%r14),%rcx
   8c60e:	41 0f 10 86 1f 03 00 	movups 0x31f(%r14),%xmm0
   8c615:	00 
   8c616:	0f 29 84 24 00 08 00 	movaps %xmm0,0x800(%rsp)
   8c61d:	00 
   8c61e:	41 0f 10 86 2f 03 00 	movups 0x32f(%r14),%xmm0
   8c625:	00 
   8c626:	0f 29 84 24 10 08 00 	movaps %xmm0,0x810(%rsp)
   8c62d:	00 
   8c62e:	41 0f 10 86 47 03 00 	movups 0x347(%r14),%xmm0
   8c635:	00 
   8c636:	48 89 94 24 20 08 00 	mov    %rdx,0x820(%rsp)
   8c63d:	00 
   8c63e:	0f 11 84 24 28 08 00 	movups %xmm0,0x828(%rsp)
   8c645:	00 
   8c646:	41 0f 10 86 60 03 00 	movups 0x360(%r14),%xmm0
   8c64d:	00 
   8c64e:	48 89 b4 24 38 08 00 	mov    %rsi,0x838(%rsp)
   8c655:	00 
   8c656:	48 89 bc 24 40 08 00 	mov    %rdi,0x840(%rsp)
   8c65d:	00 
   8c65e:	0f 11 84 24 48 08 00 	movups %xmm0,0x848(%rsp)
   8c665:	00 
   8c666:	41 0f 10 86 70 03 00 	movups 0x370(%r14),%xmm0
   8c66d:	00 
   8c66e:	0f 11 84 24 58 08 00 	movups %xmm0,0x858(%rsp)
   8c675:	00 
   8c676:	41 0f 10 86 80 03 00 	movups 0x380(%r14),%xmm0
   8c67d:	00 
   8c67e:	0f 11 84 24 68 08 00 	movups %xmm0,0x868(%rsp)
   8c685:	00 
   8c686:	41 0f 10 86 98 03 00 	movups 0x398(%r14),%xmm0
   8c68d:	00 
   8c68e:	4c 89 84 24 78 08 00 	mov    %r8,0x878(%rsp)
   8c695:	00 
   8c696:	0f 29 84 24 80 08 00 	movaps %xmm0,0x880(%rsp)
   8c69d:	00 
   8c69e:	41 0f 10 86 a9 03 00 	movups 0x3a9(%r14),%xmm0
   8c6a5:	00 
   8c6a6:	4c 89 8c 24 90 08 00 	mov    %r9,0x890(%rsp)
   8c6ad:	00 
   8c6ae:	0f 11 84 24 98 08 00 	movups %xmm0,0x898(%rsp)
   8c6b5:	00 
   8c6b6:	41 0f 10 86 c1 03 00 	movups 0x3c1(%r14),%xmm0
   8c6bd:	00 
   8c6be:	4c 89 94 24 a8 08 00 	mov    %r10,0x8a8(%rsp)
   8c6c5:	00 
   8c6c6:	0f 29 84 24 b0 08 00 	movaps %xmm0,0x8b0(%rsp)
   8c6cd:	00 
   8c6ce:	41 0f 10 86 d1 03 00 	movups 0x3d1(%r14),%xmm0
   8c6d5:	00 
   8c6d6:	0f 29 84 24 c0 08 00 	movaps %xmm0,0x8c0(%rsp)
   8c6dd:	00 
   8c6de:	41 0f 10 86 e1 03 00 	movups 0x3e1(%r14),%xmm0
   8c6e5:	00 
   8c6e6:	0f 29 84 24 d0 08 00 	movaps %xmm0,0x8d0(%rsp)
   8c6ed:	00 
   8c6ee:	41 0f 10 86 f1 03 00 	movups 0x3f1(%r14),%xmm0
   8c6f5:	00 
   8c6f6:	0f 29 84 24 e0 08 00 	movaps %xmm0,0x8e0(%rsp)
   8c6fd:	00 
   8c6fe:	41 0f 10 86 09 04 00 	movups 0x409(%r14),%xmm0
   8c705:	00 
   8c706:	48 89 8c 24 f0 08 00 	mov    %rcx,0x8f0(%rsp)
   8c70d:	00 
   8c70e:	0f 11 84 24 f8 08 00 	movups %xmm0,0x8f8(%rsp)
   8c715:	00 
   8c716:	41 0f 10 86 19 04 00 	movups 0x419(%r14),%xmm0
   8c71d:	00 
   8c71e:	49 8b 8e 49 04 00 00 	mov    0x449(%r14),%rcx
   8c725:	49 8b 96 61 04 00 00 	mov    0x461(%r14),%rdx
   8c72c:	41 0f b6 b6 69 04 00 	movzbl 0x469(%r14),%esi
   8c733:	00 
   8c734:	41 0f b7 be 6a 04 00 	movzwl 0x46a(%r14),%edi
   8c73b:	00 
   8c73c:	c1 e7 08             	shl    $0x8,%edi
   8c73f:	48 09 f7             	or     %rsi,%rdi
   8c742:	0f 11 84 24 08 09 00 	movups %xmm0,0x908(%rsp)
   8c749:	00 
   8c74a:	41 0f 10 86 29 04 00 	movups 0x429(%r14),%xmm0
   8c751:	00 
   8c752:	0f 11 84 24 18 09 00 	movups %xmm0,0x918(%rsp)
   8c759:	00 
   8c75a:	41 0f 10 86 39 04 00 	movups 0x439(%r14),%xmm0
   8c761:	00 
   8c762:	0f 11 84 24 28 09 00 	movups %xmm0,0x928(%rsp)
   8c769:	00 
   8c76a:	41 0f 10 86 51 04 00 	movups 0x451(%r14),%xmm0
   8c771:	00 
   8c772:	48 89 8c 24 38 09 00 	mov    %rcx,0x938(%rsp)
   8c779:	00 
   8c77a:	0f 29 84 24 40 09 00 	movaps %xmm0,0x940(%rsp)
   8c781:	00 
   8c782:	41 0f 10 86 6c 04 00 	movups 0x46c(%r14),%xmm0
   8c789:	00 
   8c78a:	48 89 94 24 50 09 00 	mov    %rdx,0x950(%rsp)
   8c791:	00 
   8c792:	48 89 bc 24 58 09 00 	mov    %rdi,0x958(%rsp)
   8c799:	00 
   8c79a:	0f 29 84 24 60 09 00 	movaps %xmm0,0x960(%rsp)
   8c7a1:	00 
   8c7a2:	41 0f 10 86 7c 04 00 	movups 0x47c(%r14),%xmm0
   8c7a9:	00 
   8c7aa:	41 0f b7 8e 8c 04 00 	movzwl 0x48c(%r14),%ecx
   8c7b1:	00 
   8c7b2:	0f 29 84 24 70 09 00 	movaps %xmm0,0x970(%rsp)
   8c7b9:	00 
   8c7ba:	41 0f 10 86 8e 04 00 	movups 0x48e(%r14),%xmm0
   8c7c1:	00 
   8c7c2:	48 89 8c 24 80 09 00 	mov    %rcx,0x980(%rsp)
   8c7c9:	00 
   8c7ca:	0f 11 84 24 88 09 00 	movups %xmm0,0x988(%rsp)
   8c7d1:	00 
   8c7d2:	41 0f 10 86 9e 04 00 	movups 0x49e(%r14),%xmm0
   8c7d9:	00 
   8c7da:	41 0f b6 8e ae 04 00 	movzbl 0x4ae(%r14),%ecx
   8c7e1:	00 
   8c7e2:	0f 11 84 24 98 09 00 	movups %xmm0,0x998(%rsp)
   8c7e9:	00 
   8c7ea:	41 0f 10 86 af 04 00 	movups 0x4af(%r14),%xmm0
   8c7f1:	00 
   8c7f2:	88 8c 24 a8 09 00 00 	mov    %cl,0x9a8(%rsp)
   8c7f9:	0f 29 84 24 b0 09 00 	movaps %xmm0,0x9b0(%rsp)
   8c800:	00 
   8c801:	41 0f 10 86 bf 04 00 	movups 0x4bf(%r14),%xmm0
   8c808:	00 
   8c809:	0f 29 84 24 c0 09 00 	movaps %xmm0,0x9c0(%rsp)
   8c810:	00 
   8c811:	41 0f 10 86 cf 04 00 	movups 0x4cf(%r14),%xmm0
   8c818:	00 
   8c819:	0f 29 84 24 d0 09 00 	movaps %xmm0,0x9d0(%rsp)
   8c820:	00 
   8c821:	4d 8b 16             	mov    (%r14),%r10
   8c824:	49 8b 8e df 04 00 00 	mov    0x4df(%r14),%rcx
   8c82b:	4d 8b 86 14 01 00 00 	mov    0x114(%r14),%r8
   8c832:	49 8b ae 1c 01 00 00 	mov    0x11c(%r14),%rbp
   8c839:	49 8b 56 6a          	mov    0x6a(%r14),%rdx
   8c83d:	49 8b b6 b5 01 00 00 	mov    0x1b5(%r14),%rsi
   8c844:	4d 8b 8e bc 00 00 00 	mov    0xbc(%r14),%r9
   8c84b:	48 89 8c 24 e0 09 00 	mov    %rcx,0x9e0(%rsp)
   8c852:	00 
   8c853:	49 8b be de 01 00 00 	mov    0x1de(%r14),%rdi
   8c85a:	84 c0                	test   %al,%al
   8c85c:	0f 84 9f 00 00 00    	je     8c901 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x14b1>
   8c862:	48 8b 47 08          	mov    0x8(%rdi),%rax
   8c866:	48 3b 47 10          	cmp    0x10(%rdi),%rax
   8c86a:	73 13                	jae    8c87f <_Wfl.fl_vm_exec$a5081fe92876eb37+0x142f>
   8c86c:	48 8b 0f             	mov    (%rdi),%rcx
   8c86f:	44 88 1c 01          	mov    %r11b,(%rcx,%rax,1)
   8c873:	48 ff c0             	inc    %rax
   8c876:	48 89 47 08          	mov    %rax,0x8(%rdi)
   8c87a:	e9 82 00 00 00       	jmp    8c901 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x14b1>
   8c87f:	44 88 5c 24 40       	mov    %r11b,0x40(%rsp)
   8c884:	4c 89 8c 24 00 01 00 	mov    %r9,0x100(%rsp)
   8c88b:	00 
   8c88c:	48 89 b4 24 f8 00 00 	mov    %rsi,0xf8(%rsp)
   8c893:	00 
   8c894:	48 8d 74 24 40       	lea    0x40(%rsp),%rsi
   8c899:	4c 89 ac 24 80 00 00 	mov    %r13,0x80(%rsp)
   8c8a0:	00 
   8c8a1:	48 89 9c 24 10 01 00 	mov    %rbx,0x110(%rsp)
   8c8a8:	00 
   8c8a9:	4c 89 a4 24 08 01 00 	mov    %r12,0x108(%rsp)
   8c8b0:	00 
   8c8b1:	4d 89 fc             	mov    %r15,%r12
   8c8b4:	49 89 ef             	mov    %rbp,%r15
   8c8b7:	4c 89 d5             	mov    %r10,%rbp
   8c8ba:	4d 89 c5             	mov    %r8,%r13
   8c8bd:	48 89 d3             	mov    %rdx,%rbx
   8c8c0:	e8 8b 21 0c 00       	call   14ea50 <__wolf_rt_list_push>
   8c8c5:	48 8b b4 24 f8 00 00 	mov    0xf8(%rsp),%rsi
   8c8cc:	00 
   8c8cd:	4c 8b 8c 24 00 01 00 	mov    0x100(%rsp),%r9
   8c8d4:	00 
   8c8d5:	48 89 da             	mov    %rbx,%rdx
   8c8d8:	4d 89 e8             	mov    %r13,%r8
   8c8db:	49 89 ea             	mov    %rbp,%r10
   8c8de:	4c 89 fd             	mov    %r15,%rbp
   8c8e1:	4d 89 e7             	mov    %r12,%r15
   8c8e4:	4c 8b a4 24 08 01 00 	mov    0x108(%rsp),%r12
   8c8eb:	00 
   8c8ec:	48 8b 9c 24 10 01 00 	mov    0x110(%rsp),%rbx
   8c8f3:	00 
   8c8f4:	4c 8b ac 24 80 00 00 	mov    0x80(%rsp),%r13
   8c8fb:	00 
   8c8fc:	4c 8b 5c 24 28       	mov    0x28(%rsp),%r11
   8c901:	48 8b 44 24 50       	mov    0x50(%rsp),%rax
   8c906:	83 f8 0a             	cmp    $0xa,%eax
   8c909:	0f 87 b3 05 00 00    	ja     8cec2 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1a72>
   8c90f:	89 c0                	mov    %eax,%eax
   8c911:	48 8d 0d b8 77 f8 ff 	lea    -0x78848(%rip),%rcx        # 140d0 <__abi_tag+0x13dd4>
   8c918:	48 63 04 81          	movslq (%rcx,%rax,4),%rax
   8c91c:	48 01 c8             	add    %rcx,%rax
   8c91f:	4c 8b 24 24          	mov    (%rsp),%r12
   8c923:	ff e0                	jmp    *%rax
   8c925:	4d 3b 6f 08          	cmp    0x8(%r15),%r13
   8c929:	48 8b 6c 24 08       	mov    0x8(%rsp),%rbp
   8c92e:	0f 83 07 51 00 00    	jae    91a3b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x65eb>
   8c934:	49 8b 0f             	mov    (%r15),%rcx
   8c937:	48 8b 74 24 18       	mov    0x18(%rsp),%rsi
   8c93c:	48 8b 44 31 18       	mov    0x18(%rcx,%rsi,1),%rax
   8c941:	48 89 c2             	mov    %rax,%rdx
   8c944:	48 83 c2 02          	add    $0x2,%rdx
   8c948:	0f 80 dc 4a 00 00    	jo     9142a <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5fda>
   8c94e:	48 89 54 31 18       	mov    %rdx,0x18(%rcx,%rsi,1)
   8c953:	4d 3b 62 08          	cmp    0x8(%r10),%r12
   8c957:	4c 8b 44 24 10       	mov    0x10(%rsp),%r8
   8c95c:	4c 8b 4c 24 20       	mov    0x20(%rsp),%r9
   8c961:	4c 8b 5c 24 38       	mov    0x38(%rsp),%r11
   8c966:	0f 83 2d 4f 00 00    	jae    91899 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6449>
   8c96c:	49 8b 0a             	mov    (%r10),%rcx
   8c96f:	4a 8b 54 01 61       	mov    0x61(%rcx,%r8,1),%rdx
   8c974:	48 8b 72 08          	mov    0x8(%rdx),%rsi
   8c978:	48 39 f0             	cmp    %rsi,%rax
   8c97b:	0f 83 18 4f 00 00    	jae    91899 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6449>
   8c981:	48 8d 78 01          	lea    0x1(%rax),%rdi
   8c985:	48 39 f7             	cmp    %rsi,%rdi
   8c988:	0f 83 0b 4f 00 00    	jae    91899 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6449>
   8c98e:	4a 8b 4c 01 29       	mov    0x29(%rcx,%r8,1),%rcx
   8c993:	48 8b 12             	mov    (%rdx),%rdx
   8c996:	0f b7 04 02          	movzwl (%rdx,%rax,1),%eax
   8c99a:	48 3b 41 08          	cmp    0x8(%rcx),%rax
   8c99e:	0f 83 10 51 00 00    	jae    91ab4 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6664>
   8c9a4:	49 81 f9 00 40 00 00 	cmp    $0x4000,%r9
   8c9ab:	0f 8d 04 05 00 00    	jge    8ceb5 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1a65>
   8c9b1:	4d 3b 4b 08          	cmp    0x8(%r11),%r9
   8c9b5:	0f 83 17 52 00 00    	jae    91bd2 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6782>
   8c9bb:	48 8b 09             	mov    (%rcx),%rcx
   8c9be:	48 8d 04 40          	lea    (%rax,%rax,2),%rax
   8c9c2:	0f 10 04 c1          	movups (%rcx,%rax,8),%xmm0
   8c9c6:	f2 0f 10 4c c1 10    	movsd  0x10(%rcx,%rax,8),%xmm1
   8c9cc:	49 8b 03             	mov    (%r11),%rax
   8c9cf:	4b 8d 0c 49          	lea    (%r9,%r9,2),%rcx
   8c9d3:	0f 11 04 c8          	movups %xmm0,(%rax,%rcx,8)
   8c9d7:	f2 0f 11 4c c8 10    	movsd  %xmm1,0x10(%rax,%rcx,8)
   8c9dd:	49 ff c1             	inc    %r9
   8c9e0:	4d 89 8e ac 00 00 00 	mov    %r9,0xac(%r14)
   8c9e7:	e9 6f eb ff ff       	jmp    8b55b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x10b>
   8c9ec:	4d 3b 6f 08          	cmp    0x8(%r15),%r13
   8c9f0:	48 8b 6c 24 08       	mov    0x8(%rsp),%rbp
   8c9f5:	0f 83 8d 50 00 00    	jae    91a88 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6638>
   8c9fb:	49 8b 0f             	mov    (%r15),%rcx
   8c9fe:	48 8b 74 24 18       	mov    0x18(%rsp),%rsi
   8ca03:	48 8b 44 31 18       	mov    0x18(%rcx,%rsi,1),%rax
   8ca08:	48 89 c2             	mov    %rax,%rdx
   8ca0b:	48 ff c2             	inc    %rdx
   8ca0e:	0f 80 9a 4a 00 00    	jo     914ae <_Wfl.fl_vm_exec$a5081fe92876eb37+0x605e>
   8ca14:	48 89 54 31 18       	mov    %rdx,0x18(%rcx,%rsi,1)
   8ca19:	4d 3b 62 08          	cmp    0x8(%r10),%r12
   8ca1d:	48 8b 54 24 10       	mov    0x10(%rsp),%rdx
   8ca22:	48 8b 74 24 20       	mov    0x20(%rsp),%rsi
   8ca27:	48 8b 7c 24 38       	mov    0x38(%rsp),%rdi
   8ca2c:	0f 83 4e 4f 00 00    	jae    91980 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6530>
   8ca32:	49 8b 0a             	mov    (%r10),%rcx
   8ca35:	48 8b 4c 11 61       	mov    0x61(%rcx,%rdx,1),%rcx
   8ca3a:	48 3b 41 08          	cmp    0x8(%rcx),%rax
   8ca3e:	0f 83 3c 4f 00 00    	jae    91980 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6530>
   8ca44:	48 8b 09             	mov    (%rcx),%rcx
   8ca47:	0f b6 0c 01          	movzbl (%rcx,%rax,1),%ecx
   8ca4b:	48 8d 81 00 ff ff ff 	lea    -0x100(%rcx),%rax
   8ca52:	84 c9                	test   %cl,%cl
   8ca54:	48 0f 49 c1          	cmovns %rcx,%rax
   8ca58:	48 81 fe 00 40 00 00 	cmp    $0x4000,%rsi
   8ca5f:	0f 8d 50 04 00 00    	jge    8ceb5 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1a65>
   8ca65:	48 3b 77 08          	cmp    0x8(%rdi),%rsi
   8ca69:	0f 83 76 51 00 00    	jae    91be5 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6795>
   8ca6f:	48 8b 0f             	mov    (%rdi),%rcx
   8ca72:	48 8d 14 76          	lea    (%rsi,%rsi,2),%rdx
   8ca76:	48 c7 04 d1 02 00 00 	movq   $0x2,(%rcx,%rdx,8)
   8ca7d:	00 
   8ca7e:	48 89 44 d1 08       	mov    %rax,0x8(%rcx,%rdx,8)
   8ca83:	48 c7 44 d1 10 00 00 	movq   $0x0,0x10(%rcx,%rdx,8)
   8ca8a:	00 00 
   8ca8c:	e9 ac 00 00 00       	jmp    8cb3d <_Wfl.fl_vm_exec$a5081fe92876eb37+0x16ed>
   8ca91:	4d 3b 6f 08          	cmp    0x8(%r15),%r13
   8ca95:	48 8b 6c 24 08       	mov    0x8(%rsp),%rbp
   8ca9a:	0f 83 d2 4f 00 00    	jae    91a72 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6622>
   8caa0:	49 8b 0f             	mov    (%r15),%rcx
   8caa3:	48 8b 74 24 18       	mov    0x18(%rsp),%rsi
   8caa8:	48 8b 44 31 18       	mov    0x18(%rcx,%rsi,1),%rax
   8caad:	48 89 c2             	mov    %rax,%rdx
   8cab0:	48 ff c2             	inc    %rdx
   8cab3:	0f 80 5b 49 00 00    	jo     91414 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5fc4>
   8cab9:	48 89 54 31 18       	mov    %rdx,0x18(%rcx,%rsi,1)
   8cabe:	4d 3b 62 08          	cmp    0x8(%r10),%r12
   8cac2:	48 8b 54 24 10       	mov    0x10(%rsp),%rdx
   8cac7:	48 8b 74 24 20       	mov    0x20(%rsp),%rsi
   8cacc:	48 8b 7c 24 38       	mov    0x38(%rsp),%rdi
   8cad1:	0f 83 93 4e 00 00    	jae    9196a <_Wfl.fl_vm_exec$a5081fe92876eb37+0x651a>
   8cad7:	49 8b 0a             	mov    (%r10),%rcx
   8cada:	48 8b 4c 11 61       	mov    0x61(%rcx,%rdx,1),%rcx
   8cadf:	48 3b 41 08          	cmp    0x8(%rcx),%rax
   8cae3:	0f 83 81 4e 00 00    	jae    9196a <_Wfl.fl_vm_exec$a5081fe92876eb37+0x651a>
   8cae9:	48 8b 09             	mov    (%rcx),%rcx
   8caec:	0f b6 04 01          	movzbl (%rcx,%rax,1),%eax
   8caf0:	48 03 44 24 68       	add    0x68(%rsp),%rax
   8caf5:	0f 80 ea 49 00 00    	jo     914e5 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6095>
   8cafb:	48 8b 4f 08          	mov    0x8(%rdi),%rcx
   8caff:	48 39 c8             	cmp    %rcx,%rax
   8cb02:	0f 83 25 50 00 00    	jae    91b2d <_Wfl.fl_vm_exec$a5081fe92876eb37+0x66dd>
   8cb08:	48 81 fe 00 40 00 00 	cmp    $0x4000,%rsi
   8cb0f:	0f 8d a0 03 00 00    	jge    8ceb5 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1a65>
   8cb15:	48 39 ce             	cmp    %rcx,%rsi
   8cb18:	0f 83 e8 50 00 00    	jae    91c06 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x67b6>
   8cb1e:	48 8b 0f             	mov    (%rdi),%rcx
   8cb21:	48 8d 04 40          	lea    (%rax,%rax,2),%rax
   8cb25:	0f 10 04 c1          	movups (%rcx,%rax,8),%xmm0
   8cb29:	f2 0f 10 4c c1 10    	movsd  0x10(%rcx,%rax,8),%xmm1
   8cb2f:	48 8d 04 76          	lea    (%rsi,%rsi,2),%rax
   8cb33:	0f 11 04 c1          	movups %xmm0,(%rcx,%rax,8)
   8cb37:	f2 0f 11 4c c1 10    	movsd  %xmm1,0x10(%rcx,%rax,8)
   8cb3d:	48 ff c6             	inc    %rsi
   8cb40:	49 89 b6 ac 00 00 00 	mov    %rsi,0xac(%r14)
   8cb47:	e9 0f ea ff ff       	jmp    8b55b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x10b>
   8cb4c:	48 8b 54 24 20       	mov    0x20(%rsp),%rdx
   8cb51:	48 81 fa 00 40 00 00 	cmp    $0x4000,%rdx
   8cb58:	48 8b 6c 24 08       	mov    0x8(%rsp),%rbp
   8cb5d:	0f 8d 52 03 00 00    	jge    8ceb5 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1a65>
   8cb63:	48 8b 44 24 38       	mov    0x38(%rsp),%rax
   8cb68:	48 3b 50 08          	cmp    0x8(%rax),%rdx
   8cb6c:	0f 83 b5 50 00 00    	jae    91c27 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x67d7>
   8cb72:	48 8b 00             	mov    (%rax),%rax
   8cb75:	48 8d 0c 52          	lea    (%rdx,%rdx,2),%rcx
   8cb79:	48 c7 04 c8 01 00 00 	movq   $0x1,(%rax,%rcx,8)
   8cb80:	00 
   8cb81:	48 c7 44 c8 08 01 00 	movq   $0x1,0x8(%rax,%rcx,8)
   8cb88:	00 00 
   8cb8a:	48 c7 44 c8 10 00 00 	movq   $0x0,0x10(%rax,%rcx,8)
   8cb91:	00 00 
   8cb93:	e9 10 05 00 00       	jmp    8d0a8 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1c58>
   8cb98:	48 8b 54 24 20       	mov    0x20(%rsp),%rdx
   8cb9d:	48 81 fa 00 40 00 00 	cmp    $0x4000,%rdx
   8cba4:	48 8b 6c 24 08       	mov    0x8(%rsp),%rbp
   8cba9:	0f 8d 06 03 00 00    	jge    8ceb5 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1a65>
   8cbaf:	48 8b 44 24 38       	mov    0x38(%rsp),%rax
   8cbb4:	48 3b 50 08          	cmp    0x8(%rax),%rdx
   8cbb8:	0f 83 8a 50 00 00    	jae    91c48 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x67f8>
   8cbbe:	48 8b 00             	mov    (%rax),%rax
   8cbc1:	48 8d 0c 52          	lea    (%rdx,%rdx,2),%rcx
   8cbc5:	48 c7 04 c8 01 00 00 	movq   $0x1,(%rax,%rcx,8)
   8cbcc:	00 
   8cbcd:	48 ff c2             	inc    %rdx
   8cbd0:	0f 57 c0             	xorps  %xmm0,%xmm0
   8cbd3:	0f 11 44 c8 08       	movups %xmm0,0x8(%rax,%rcx,8)
   8cbd8:	49 89 96 ac 00 00 00 	mov    %rdx,0xac(%r14)
   8cbdf:	e9 77 e9 ff ff       	jmp    8b55b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x10b>
   8cbe4:	4d 3b 6f 08          	cmp    0x8(%r15),%r13
   8cbe8:	0f 83 63 4e 00 00    	jae    91a51 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6601>
   8cbee:	49 8b 0f             	mov    (%r15),%rcx
   8cbf1:	48 8b 74 24 18       	mov    0x18(%rsp),%rsi
   8cbf6:	48 8b 44 31 18       	mov    0x18(%rcx,%rsi,1),%rax
   8cbfb:	48 89 c2             	mov    %rax,%rdx
   8cbfe:	48 ff c2             	inc    %rdx
   8cc01:	48 8b 6c 24 08       	mov    0x8(%rsp),%rbp
   8cc06:	0f 80 b8 48 00 00    	jo     914c4 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6074>
   8cc0c:	48 89 54 31 18       	mov    %rdx,0x18(%rcx,%rsi,1)
   8cc11:	4d 3b 62 08          	cmp    0x8(%r10),%r12
   8cc15:	48 8b 54 24 10       	mov    0x10(%rsp),%rdx
   8cc1a:	48 8b 74 24 20       	mov    0x20(%rsp),%rsi
   8cc1f:	0f 83 f8 4c 00 00    	jae    9191d <_Wfl.fl_vm_exec$a5081fe92876eb37+0x64cd>
   8cc25:	49 8b 0a             	mov    (%r10),%rcx
   8cc28:	48 8b 4c 11 61       	mov    0x61(%rcx,%rdx,1),%rcx
   8cc2d:	48 3b 41 08          	cmp    0x8(%rcx),%rax
   8cc31:	0f 83 e6 4c 00 00    	jae    9191d <_Wfl.fl_vm_exec$a5081fe92876eb37+0x64cd>
   8cc37:	48 8b 09             	mov    (%rcx),%rcx
   8cc3a:	0f b6 04 01          	movzbl (%rcx,%rax,1),%eax
   8cc3e:	48 29 c6             	sub    %rax,%rsi
   8cc41:	0f 80 d5 48 00 00    	jo     9151c <_Wfl.fl_vm_exec$a5081fe92876eb37+0x60cc>
   8cc47:	49 89 b6 ac 00 00 00 	mov    %rsi,0xac(%r14)
   8cc4e:	e9 08 e9 ff ff       	jmp    8b55b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x10b>
   8cc53:	48 8b 54 24 20       	mov    0x20(%rsp),%rdx
   8cc58:	48 81 fa 00 40 00 00 	cmp    $0x4000,%rdx
   8cc5f:	48 8b 6c 24 08       	mov    0x8(%rsp),%rbp
   8cc64:	0f 8d 4b 02 00 00    	jge    8ceb5 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1a65>
   8cc6a:	48 8b 44 24 38       	mov    0x38(%rsp),%rax
   8cc6f:	48 3b 50 08          	cmp    0x8(%rax),%rdx
   8cc73:	0f 83 22 4f 00 00    	jae    91b9b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x674b>
   8cc79:	48 8b 00             	mov    (%rax),%rax
   8cc7c:	48 8d 0c 52          	lea    (%rdx,%rdx,2),%rcx
   8cc80:	48 ff c2             	inc    %rdx
   8cc83:	0f 57 c0             	xorps  %xmm0,%xmm0
   8cc86:	0f 11 04 c8          	movups %xmm0,(%rax,%rcx,8)
   8cc8a:	48 c7 44 c8 10 00 00 	movq   $0x0,0x10(%rax,%rcx,8)
   8cc91:	00 00 
   8cc93:	49 89 96 ac 00 00 00 	mov    %rdx,0xac(%r14)
   8cc9a:	e9 bc e8 ff ff       	jmp    8b55b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x10b>
   8cc9f:	4d 3b 6f 08          	cmp    0x8(%r15),%r13
   8cca3:	0f 83 f5 4d 00 00    	jae    91a9e <_Wfl.fl_vm_exec$a5081fe92876eb37+0x664e>
   8cca9:	49 8b 0f             	mov    (%r15),%rcx
   8ccac:	48 8b 74 24 18       	mov    0x18(%rsp),%rsi
   8ccb1:	48 8b 44 31 18       	mov    0x18(%rcx,%rsi,1),%rax
   8ccb6:	48 89 c2             	mov    %rax,%rdx
   8ccb9:	48 ff c2             	inc    %rdx
   8ccbc:	48 8b 6c 24 08       	mov    0x8(%rsp),%rbp
   8ccc1:	0f 80 d1 47 00 00    	jo     91498 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6048>
   8ccc7:	48 89 54 31 18       	mov    %rdx,0x18(%rcx,%rsi,1)
   8cccc:	4d 3b 62 08          	cmp    0x8(%r10),%r12
   8ccd0:	48 8b 54 24 10       	mov    0x10(%rsp),%rdx
   8ccd5:	0f 83 63 4c 00 00    	jae    9193e <_Wfl.fl_vm_exec$a5081fe92876eb37+0x64ee>
   8ccdb:	49 8b 0a             	mov    (%r10),%rcx
   8ccde:	48 8b 4c 11 61       	mov    0x61(%rcx,%rdx,1),%rcx
   8cce3:	48 3b 41 08          	cmp    0x8(%rcx),%rax
   8cce7:	0f 83 51 4c 00 00    	jae    9193e <_Wfl.fl_vm_exec$a5081fe92876eb37+0x64ee>
   8cced:	48 8b 09             	mov    (%rcx),%rcx
   8ccf0:	0f b6 1c 01          	movzbl (%rcx,%rax,1),%ebx
   8ccf4:	48 85 db             	test   %rbx,%rbx
   8ccf7:	75 18                	jne    8cd11 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x18c1>
   8ccf9:	e9 5d e8 ff ff       	jmp    8b55b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x10b>
   8ccfe:	66 90                	xchg   %ax,%ax
   8cd00:	4c 89 f7             	mov    %r14,%rdi
   8cd03:	e8 58 a8 02 00       	call   b7560 <_Wfl.fl_push_overflow$ec8cb5daf6580c79>
   8cd08:	48 ff cb             	dec    %rbx
   8cd0b:	0f 84 4a e8 ff ff    	je     8b55b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x10b>
   8cd11:	49 8b 86 ac 00 00 00 	mov    0xac(%r14),%rax
   8cd18:	48 3d 00 40 00 00    	cmp    $0x4000,%rax
   8cd1e:	7d e0                	jge    8cd00 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x18b0>
   8cd20:	49 8b 8e a4 00 00 00 	mov    0xa4(%r14),%rcx
   8cd27:	48 3b 41 08          	cmp    0x8(%rcx),%rax
   8cd2b:	0f 83 e7 49 00 00    	jae    91718 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x62c8>
   8cd31:	48 8b 09             	mov    (%rcx),%rcx
   8cd34:	48 8d 14 40          	lea    (%rax,%rax,2),%rdx
   8cd38:	48 ff c0             	inc    %rax
   8cd3b:	0f 57 c0             	xorps  %xmm0,%xmm0
   8cd3e:	0f 11 04 d1          	movups %xmm0,(%rcx,%rdx,8)
   8cd42:	48 c7 44 d1 10 00 00 	movq   $0x0,0x10(%rcx,%rdx,8)
   8cd49:	00 00 
   8cd4b:	49 89 86 ac 00 00 00 	mov    %rax,0xac(%r14)
   8cd52:	48 ff cb             	dec    %rbx
   8cd55:	75 ba                	jne    8cd11 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x18c1>
   8cd57:	e9 ff e7 ff ff       	jmp    8b55b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x10b>
   8cd5c:	48 83 7c 24 20 00    	cmpq   $0x0,0x20(%rsp)
   8cd62:	48 8b 6c 24 08       	mov    0x8(%rsp),%rbp
   8cd67:	7f 14                	jg     8cd7d <_Wfl.fl_vm_exec$a5081fe92876eb37+0x192d>
   8cd69:	ba 16 00 00 00       	mov    $0x16,%edx
   8cd6e:	4c 89 f7             	mov    %r14,%rdi
   8cd71:	48 8d 35 6b 7b f8 ff 	lea    -0x78495(%rip),%rsi        # 148e3 <__abi_tag+0x145e7>
   8cd78:	e8 83 c6 02 00       	call   b9400 <_Wfl.fl_current_bug$22c9f442c8960c68>
   8cd7d:	49 8b 86 ac 00 00 00 	mov    0xac(%r14),%rax
   8cd84:	48 ff c8             	dec    %rax
   8cd87:	0f 80 b3 46 00 00    	jo     91440 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5ff0>
   8cd8d:	49 89 86 ac 00 00 00 	mov    %rax,0xac(%r14)
   8cd94:	49 8b 8e a4 00 00 00 	mov    0xa4(%r14),%rcx
   8cd9b:	48 3b 41 08          	cmp    0x8(%rcx),%rax
   8cd9f:	0f 82 b6 e7 ff ff    	jb     8b55b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x10b>
   8cda5:	e9 af 4d 00 00       	jmp    91b59 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6709>
   8cdaa:	4d 3b 6f 08          	cmp    0x8(%r15),%r13
   8cdae:	48 8b 6c 24 08       	mov    0x8(%rsp),%rbp
   8cdb3:	0f 83 6c 4c 00 00    	jae    91a25 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x65d5>
   8cdb9:	49 8b 0f             	mov    (%r15),%rcx
   8cdbc:	48 8b 74 24 18       	mov    0x18(%rsp),%rsi
   8cdc1:	48 8b 44 31 18       	mov    0x18(%rcx,%rsi,1),%rax
   8cdc6:	48 89 c2             	mov    %rax,%rdx
   8cdc9:	48 ff c2             	inc    %rdx
   8cdcc:	0f 80 b0 46 00 00    	jo     91482 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6032>
   8cdd2:	48 89 54 31 18       	mov    %rdx,0x18(%rcx,%rsi,1)
   8cdd7:	4d 3b 62 08          	cmp    0x8(%r10),%r12
   8cddb:	48 8b 54 24 10       	mov    0x10(%rsp),%rdx
   8cde0:	48 8b 74 24 20       	mov    0x20(%rsp),%rsi
   8cde5:	0f 83 69 4b 00 00    	jae    91954 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6504>
   8cdeb:	49 8b 0a             	mov    (%r10),%rcx
   8cdee:	48 8b 4c 11 61       	mov    0x61(%rcx,%rdx,1),%rcx
   8cdf3:	48 3b 41 08          	cmp    0x8(%rcx),%rax
   8cdf7:	0f 83 57 4b 00 00    	jae    91954 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6504>
   8cdfd:	48 8b 09             	mov    (%rcx),%rcx
   8ce00:	0f b6 1c 01          	movzbl (%rcx,%rax,1),%ebx
   8ce04:	48 85 f6             	test   %rsi,%rsi
   8ce07:	7f 14                	jg     8ce1d <_Wfl.fl_vm_exec$a5081fe92876eb37+0x19cd>
   8ce09:	ba 16 00 00 00       	mov    $0x16,%edx
   8ce0e:	4c 89 f7             	mov    %r14,%rdi
   8ce11:	48 8d 35 cb 7a f8 ff 	lea    -0x78535(%rip),%rsi        # 148e3 <__abi_tag+0x145e7>
   8ce18:	e8 e3 c5 02 00       	call   b9400 <_Wfl.fl_current_bug$22c9f442c8960c68>
   8ce1d:	49 8b 86 ac 00 00 00 	mov    0xac(%r14),%rax
   8ce24:	48 ff c8             	dec    %rax
   8ce27:	0f 80 ce 46 00 00    	jo     914fb <_Wfl.fl_vm_exec$a5081fe92876eb37+0x60ab>
   8ce2d:	49 89 86 ac 00 00 00 	mov    %rax,0xac(%r14)
   8ce34:	49 8b 8e a4 00 00 00 	mov    0xa4(%r14),%rcx
   8ce3b:	48 8b 51 08          	mov    0x8(%rcx),%rdx
   8ce3f:	48 39 d0             	cmp    %rdx,%rax
   8ce42:	0f 83 82 4c 00 00    	jae    91aca <_Wfl.fl_vm_exec$a5081fe92876eb37+0x667a>
   8ce48:	48 03 5c 24 68       	add    0x68(%rsp),%rbx
   8ce4d:	0f 80 ea 46 00 00    	jo     9153d <_Wfl.fl_vm_exec$a5081fe92876eb37+0x60ed>
   8ce53:	48 39 d3             	cmp    %rdx,%rbx
   8ce56:	0f 83 8f 4c 00 00    	jae    91aeb <_Wfl.fl_vm_exec$a5081fe92876eb37+0x669b>
   8ce5c:	48 8b 09             	mov    (%rcx),%rcx
   8ce5f:	48 8d 04 40          	lea    (%rax,%rax,2),%rax
   8ce63:	0f 10 04 c1          	movups (%rcx,%rax,8),%xmm0
   8ce67:	f2 0f 10 4c c1 10    	movsd  0x10(%rcx,%rax,8),%xmm1
   8ce6d:	48 8d 04 5b          	lea    (%rbx,%rbx,2),%rax
   8ce71:	0f 11 04 c1          	movups %xmm0,(%rcx,%rax,8)
   8ce75:	f2 0f 11 4c c1 10    	movsd  %xmm1,0x10(%rcx,%rax,8)
   8ce7b:	e9 db e6 ff ff       	jmp    8b55b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x10b>
   8ce80:	48 8b 54 24 20       	mov    0x20(%rsp),%rdx
   8ce85:	48 89 d0             	mov    %rdx,%rax
   8ce88:	48 ff c8             	dec    %rax
   8ce8b:	48 8b 6c 24 08       	mov    0x8(%rsp),%rbp
   8ce90:	0f 80 cb 45 00 00    	jo     91461 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6011>
   8ce96:	48 8b 74 24 38       	mov    0x38(%rsp),%rsi
   8ce9b:	48 8b 4e 08          	mov    0x8(%rsi),%rcx
   8ce9f:	48 39 c8             	cmp    %rcx,%rax
   8cea2:	0f 83 9b 4c 00 00    	jae    91b43 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x66f3>
   8cea8:	48 81 fa 00 40 00 00 	cmp    $0x4000,%rdx
   8ceaf:	0f 8c cb 01 00 00    	jl     8d080 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1c30>
   8ceb5:	4c 89 f7             	mov    %r14,%rdi
   8ceb8:	e8 a3 a6 02 00       	call   b7560 <_Wfl.fl_push_overflow$ec8cb5daf6580c79>
   8cebd:	e9 99 e6 ff ff       	jmp    8b55b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x10b>
   8cec2:	41 8d 43 f5          	lea    -0xb(%r11),%eax
   8cec6:	3c 01                	cmp    $0x1,%al
   8cec8:	0f 87 2d 01 00 00    	ja     8cffb <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1bab>
   8cece:	4d 3b 6f 08          	cmp    0x8(%r15),%r13
   8ced2:	0f 83 91 4d 00 00    	jae    91c69 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6819>
   8ced8:	49 8b 07             	mov    (%r15),%rax
   8cedb:	48 8b 7c 24 18       	mov    0x18(%rsp),%rdi
   8cee0:	48 8b 54 38 18       	mov    0x18(%rax,%rdi,1),%rdx
   8cee5:	48 89 d1             	mov    %rdx,%rcx
   8cee8:	48 ff c1             	inc    %rcx
   8ceeb:	48 8b 6c 24 08       	mov    0x8(%rsp),%rbp
   8cef0:	4c 8b 44 24 68       	mov    0x68(%rsp),%r8
   8cef5:	48 8b 34 24          	mov    (%rsp),%rsi
   8cef9:	4c 8b 4c 24 20       	mov    0x20(%rsp),%r9
   8cefe:	0f 80 4c 46 00 00    	jo     91550 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6100>
   8cf04:	48 89 4c 38 18       	mov    %rcx,0x18(%rax,%rdi,1)
   8cf09:	49 8b 4a 08          	mov    0x8(%r10),%rcx
   8cf0d:	48 39 ce             	cmp    %rcx,%rsi
   8cf10:	48 8b 74 24 10       	mov    0x10(%rsp),%rsi
   8cf15:	0f 83 fc 4b 00 00    	jae    91b17 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x66c7>
   8cf1b:	49 8b 02             	mov    (%r10),%rax
   8cf1e:	48 8b 74 30 61       	mov    0x61(%rax,%rsi,1),%rsi
   8cf23:	48 3b 56 08          	cmp    0x8(%rsi),%rdx
   8cf27:	0f 83 ea 4b 00 00    	jae    91b17 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x66c7>
   8cf2d:	48 8b 7c 24 78       	mov    0x78(%rsp),%rdi
   8cf32:	48 39 cf             	cmp    %rcx,%rdi
   8cf35:	0f 83 c6 4b 00 00    	jae    91b01 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x66b1>
   8cf3b:	48 8b 36             	mov    (%rsi),%rsi
   8cf3e:	0f b6 14 16          	movzbl (%rsi,%rdx,1),%edx
   8cf42:	48 69 f7 60 01 00 00 	imul   $0x160,%rdi,%rsi
   8cf49:	48 8b 74 30 29       	mov    0x29(%rax,%rsi,1),%rsi
   8cf4e:	48 39 56 08          	cmp    %rdx,0x8(%rsi)
   8cf52:	0f 86 a9 4b 00 00    	jbe    91b01 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x66b1>
   8cf58:	4c 89 44 24 68       	mov    %r8,0x68(%rsp)
   8cf5d:	48 8b 3e             	mov    (%rsi),%rdi
   8cf60:	48 8d 14 52          	lea    (%rdx,%rdx,2),%rdx
   8cf64:	48 8b 34 d7          	mov    (%rdi,%rdx,8),%rsi
   8cf68:	4c 8d 46 f4          	lea    -0xc(%rsi),%r8
   8cf6c:	49 83 f8 f8          	cmp    $0xfffffffffffffff8,%r8
   8cf70:	0f 82 86 49 00 00    	jb     918fc <_Wfl.fl_vm_exec$a5081fe92876eb37+0x64ac>
   8cf76:	48 8b 54 d7 08       	mov    0x8(%rdi,%rdx,8),%rdx
   8cf7b:	48 85 d2             	test   %rdx,%rdx
   8cf7e:	0f 88 78 49 00 00    	js     918fc <_Wfl.fl_vm_exec$a5081fe92876eb37+0x64ac>
   8cf84:	41 89 d7             	mov    %edx,%r15d
   8cf87:	49 39 cf             	cmp    %rcx,%r15
   8cf8a:	0f 8d 6c 49 00 00    	jge    918fc <_Wfl.fl_vm_exec$a5081fe92876eb37+0x64ac>
   8cf90:	49 69 df 60 01 00 00 	imul   $0x160,%r15,%rbx
   8cf97:	48 39 34 18          	cmp    %rsi,(%rax,%rbx,1)
   8cf9b:	0f 85 5b 49 00 00    	jne    918fc <_Wfl.fl_vm_exec$a5081fe92876eb37+0x64ac>
   8cfa1:	48 01 d8             	add    %rbx,%rax
   8cfa4:	48 c1 ea 20          	shr    $0x20,%rdx
   8cfa8:	48 39 50 19          	cmp    %rdx,0x19(%rax)
   8cfac:	0f 85 4a 49 00 00    	jne    918fc <_Wfl.fl_vm_exec$a5081fe92876eb37+0x64ac>
   8cfb2:	4c 8b a0 29 01 00 00 	mov    0x129(%rax),%r12
   8cfb9:	41 80 fb 0b          	cmp    $0xb,%r11b
   8cfbd:	0f 85 c6 02 00 00    	jne    8d289 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1e39>
   8cfc3:	4d 85 e4             	test   %r12,%r12
   8cfc6:	0f 88 b9 04 00 00    	js     8d485 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x2035>
   8cfcc:	48 8b 7c 24 38       	mov    0x38(%rsp),%rdi
   8cfd1:	4c 3b 67 08          	cmp    0x8(%rdi),%r12
   8cfd5:	0f 83 8b 4d 00 00    	jae    91d66 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6916>
   8cfdb:	48 8b 17             	mov    (%rdi),%rdx
   8cfde:	4b 8d 34 64          	lea    (%r12,%r12,2),%rsi
   8cfe2:	48 8d 04 f2          	lea    (%rdx,%rsi,8),%rax
   8cfe6:	48 8d 0c f2          	lea    (%rdx,%rsi,8),%rcx
   8cfea:	48 83 c1 08          	add    $0x8,%rcx
   8cfee:	48 8d 14 f2          	lea    (%rdx,%rsi,8),%rdx
   8cff2:	48 83 c2 10          	add    $0x10,%rdx
   8cff6:	e9 a3 04 00 00       	jmp    8d49e <_Wfl.fl_vm_exec$a5081fe92876eb37+0x204e>
   8cffb:	41 80 fb 0d          	cmp    $0xd,%r11b
   8cfff:	0f 85 b2 00 00 00    	jne    8d0b7 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1c67>
   8d005:	4d 3b 6f 08          	cmp    0x8(%r15),%r13
   8d009:	0f 83 e9 4c 00 00    	jae    91cf8 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x68a8>
   8d00f:	49 8b 0f             	mov    (%r15),%rcx
   8d012:	48 8b 74 24 18       	mov    0x18(%rsp),%rsi
   8d017:	48 8b 44 31 18       	mov    0x18(%rcx,%rsi,1),%rax
   8d01c:	48 89 c2             	mov    %rax,%rdx
   8d01f:	48 ff c2             	inc    %rdx
   8d022:	48 8b 6c 24 08       	mov    0x8(%rsp),%rbp
   8d027:	4c 8b 24 24          	mov    (%rsp),%r12
   8d02b:	0f 80 32 45 00 00    	jo     91563 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6113>
   8d031:	48 89 54 31 18       	mov    %rdx,0x18(%rcx,%rsi,1)
   8d036:	4d 3b 62 08          	cmp    0x8(%r10),%r12
   8d03a:	48 8b 54 24 10       	mov    0x10(%rsp),%rdx
   8d03f:	0f 83 77 4b 00 00    	jae    91bbc <_Wfl.fl_vm_exec$a5081fe92876eb37+0x676c>
   8d045:	49 8b 0a             	mov    (%r10),%rcx
   8d048:	48 8b 4c 11 61       	mov    0x61(%rcx,%rdx,1),%rcx
   8d04d:	48 3b 41 08          	cmp    0x8(%rcx),%rax
   8d051:	0f 83 65 4b 00 00    	jae    91bbc <_Wfl.fl_vm_exec$a5081fe92876eb37+0x676c>
   8d057:	48 8b 09             	mov    (%rcx),%rcx
   8d05a:	0f b6 1c 01          	movzbl (%rcx,%rax,1),%ebx
   8d05e:	48 03 5c 24 68       	add    0x68(%rsp),%rbx
   8d063:	0f 80 0d 45 00 00    	jo     91576 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6126>
   8d069:	4c 89 f7             	mov    %r14,%rdi
   8d06c:	48 89 de             	mov    %rbx,%rsi
   8d06f:	e8 2c a6 02 00       	call   b76a0 <_Wfl.fl_close_upvals$345768e8f0248c70>
   8d074:	49 89 9e ac 00 00 00 	mov    %rbx,0xac(%r14)
   8d07b:	e9 db e4 ff ff       	jmp    8b55b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x10b>
   8d080:	48 39 ca             	cmp    %rcx,%rdx
   8d083:	0f 83 f1 4a 00 00    	jae    91b7a <_Wfl.fl_vm_exec$a5081fe92876eb37+0x672a>
   8d089:	48 8b 0e             	mov    (%rsi),%rcx
   8d08c:	48 8d 04 40          	lea    (%rax,%rax,2),%rax
   8d090:	0f 10 04 c1          	movups (%rcx,%rax,8),%xmm0
   8d094:	f2 0f 10 4c c1 10    	movsd  0x10(%rcx,%rax,8),%xmm1
   8d09a:	48 8d 04 52          	lea    (%rdx,%rdx,2),%rax
   8d09e:	0f 11 04 c1          	movups %xmm0,(%rcx,%rax,8)
   8d0a2:	f2 0f 11 4c c1 10    	movsd  %xmm1,0x10(%rcx,%rax,8)
   8d0a8:	48 ff c2             	inc    %rdx
   8d0ab:	49 89 96 ac 00 00 00 	mov    %rdx,0xac(%r14)
   8d0b2:	e9 a4 e4 ff ff       	jmp    8b55b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x10b>
   8d0b7:	41 8d 43 f2          	lea    -0xe(%r11),%eax
   8d0bb:	3c 02                	cmp    $0x2,%al
   8d0bd:	0f 87 4e 02 00 00    	ja     8d311 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1ec1>
   8d0c3:	4d 3b 6f 08          	cmp    0x8(%r15),%r13
   8d0c7:	0f 83 db 4c 00 00    	jae    91da8 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6958>
   8d0cd:	48 89 d3             	mov    %rdx,%rbx
   8d0d0:	49 8b 07             	mov    (%r15),%rax
   8d0d3:	48 8b 7c 24 18       	mov    0x18(%rsp),%rdi
   8d0d8:	48 8b 4c 38 18       	mov    0x18(%rax,%rdi,1),%rcx
   8d0dd:	48 89 ca             	mov    %rcx,%rdx
   8d0e0:	48 83 c2 02          	add    $0x2,%rdx
   8d0e4:	48 8b 34 24          	mov    (%rsp),%rsi
   8d0e8:	0f 80 ca 44 00 00    	jo     915b8 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6168>
   8d0ee:	48 89 54 38 18       	mov    %rdx,0x18(%rax,%rdi,1)
   8d0f3:	49 8b 42 08          	mov    0x8(%r10),%rax
   8d0f7:	48 39 c6             	cmp    %rax,%rsi
   8d0fa:	4c 8b 4c 24 10       	mov    0x10(%rsp),%r9
   8d0ff:	0f 83 90 4b 00 00    	jae    91c95 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6845>
   8d105:	49 8b 12             	mov    (%r10),%rdx
   8d108:	4a 8b 74 0a 61       	mov    0x61(%rdx,%r9,1),%rsi
   8d10d:	48 8b 7e 08          	mov    0x8(%rsi),%rdi
   8d111:	48 39 f9             	cmp    %rdi,%rcx
   8d114:	0f 83 7b 4b 00 00    	jae    91c95 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6845>
   8d11a:	4d 89 c4             	mov    %r8,%r12
   8d11d:	4c 8d 41 01          	lea    0x1(%rcx),%r8
   8d121:	49 39 f8             	cmp    %rdi,%r8
   8d124:	0f 83 6b 4b 00 00    	jae    91c95 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6845>
   8d12a:	4a 8b 7c 0a 29       	mov    0x29(%rdx,%r9,1),%rdi
   8d12f:	48 8b 36             	mov    (%rsi),%rsi
   8d132:	0f b7 0c 0e          	movzwl (%rsi,%rcx,1),%ecx
   8d136:	48 3b 4f 08          	cmp    0x8(%rdi),%rcx
   8d13a:	0f 83 89 4c 00 00    	jae    91dc9 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6979>
   8d140:	49 89 ed             	mov    %rbp,%r13
   8d143:	48 8b 37             	mov    (%rdi),%rsi
   8d146:	48 8d 0c 49          	lea    (%rcx,%rcx,2),%rcx
   8d14a:	4c 8b 3c ce          	mov    (%rsi,%rcx,8),%r15
   8d14e:	48 8b 6c ce 08       	mov    0x8(%rsi,%rcx,8),%rbp
   8d153:	f2 0f 10 44 ce 10    	movsd  0x10(%rsi,%rcx,8),%xmm0
   8d159:	4c 89 bc 24 90 00 00 	mov    %r15,0x90(%rsp)
   8d160:	00 
   8d161:	48 89 ac 24 98 00 00 	mov    %rbp,0x98(%rsp)
   8d168:	00 
   8d169:	f2 0f 11 84 24 a0 00 	movsd  %xmm0,0xa0(%rsp)
   8d170:	00 00 
   8d172:	48 8b 4c 24 78       	mov    0x78(%rsp),%rcx
   8d177:	48 39 c1             	cmp    %rax,%rcx
   8d17a:	0f 83 95 41 00 00    	jae    91315 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5ec5>
   8d180:	48 89 6c 24 28       	mov    %rbp,0x28(%rsp)
   8d185:	4c 89 54 24 10       	mov    %r10,0x10(%rsp)
   8d18a:	48 69 f1 60 01 00 00 	imul   $0x160,%rcx,%rsi
   8d191:	48 8b 8c 32 e1 00 00 	mov    0xe1(%rdx,%rsi,1),%rcx
   8d198:	00 
   8d199:	48 8b 84 32 e9 00 00 	mov    0xe9(%rdx,%rsi,1),%rax
   8d1a0:	00 
   8d1a1:	f2 0f 10 84 32 f1 00 	movsd  0xf1(%rdx,%rsi,1),%xmm0
   8d1a8:	00 00 
   8d1aa:	48 89 8c 24 98 01 00 	mov    %rcx,0x198(%rsp)
   8d1b1:	00 
   8d1b2:	48 89 84 24 a0 01 00 	mov    %rax,0x1a0(%rsp)
   8d1b9:	00 
   8d1ba:	f2 0f 11 84 24 a8 01 	movsd  %xmm0,0x1a8(%rsp)
   8d1c1:	00 00 
   8d1c3:	83 7c 24 50 0f       	cmpl   $0xf,0x50(%rsp)
   8d1c8:	0f 84 25 20 00 00    	je     8f1f3 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x3da3>
   8d1ce:	83 7c 24 50 0e       	cmpl   $0xe,0x50(%rsp)
   8d1d3:	0f 85 7a 28 00 00    	jne    8fa53 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4603>
   8d1d9:	49 83 ff 04          	cmp    $0x4,%r15
   8d1dd:	0f 87 f1 3e 00 00    	ja     910d4 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5c84>
   8d1e3:	ba 16 00 00 00       	mov    $0x16,%edx
   8d1e8:	4c 0f a3 fa          	bt     %r15,%rdx
   8d1ec:	0f 83 e2 3e 00 00    	jae    910d4 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5c84>
   8d1f2:	48 8d 51 f4          	lea    -0xc(%rcx),%rdx
   8d1f6:	48 83 fa f8          	cmp    $0xfffffffffffffff8,%rdx
   8d1fa:	0f 82 71 55 00 00    	jb     92771 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x7321>
   8d200:	48 85 c0             	test   %rax,%rax
   8d203:	0f 88 68 55 00 00    	js     92771 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x7321>
   8d209:	89 c3                	mov    %eax,%ebx
   8d20b:	48 8b 54 24 10       	mov    0x10(%rsp),%rdx
   8d210:	48 3b 5a 08          	cmp    0x8(%rdx),%rbx
   8d214:	0f 8d 57 55 00 00    	jge    92771 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x7321>
   8d21a:	48 8b 54 24 10       	mov    0x10(%rsp),%rdx
   8d21f:	48 8b 12             	mov    (%rdx),%rdx
   8d222:	48 69 eb 60 01 00 00 	imul   $0x160,%rbx,%rbp
   8d229:	48 39 0c 2a          	cmp    %rcx,(%rdx,%rbp,1)
   8d22d:	0f 85 3e 55 00 00    	jne    92771 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x7321>
   8d233:	48 c1 e8 20          	shr    $0x20,%rax
   8d237:	48 39 44 2a 19       	cmp    %rax,0x19(%rdx,%rbp,1)
   8d23c:	0f 85 2f 55 00 00    	jne    92771 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x7321>
   8d242:	49 83 ff 04          	cmp    $0x4,%r15
   8d246:	0f 85 a2 20 00 00    	jne    8f2ee <_Wfl.fl_vm_exec$a5081fe92876eb37+0x3e9e>
   8d24c:	48 8d bc 24 c0 04 00 	lea    0x4c0(%rsp),%rdi
   8d253:	00 
   8d254:	48 8d b4 24 90 00 00 	lea    0x90(%rsp),%rsi
   8d25b:	00 
   8d25c:	e8 8f 99 03 00       	call   c6bf0 <_Wfl.fl_obj_index$6801bb8d311b3e64>
   8d261:	48 8b 4c 24 10       	mov    0x10(%rsp),%rcx
   8d266:	48 3b 41 08          	cmp    0x8(%rcx),%rax
   8d26a:	0f 83 ac 54 00 00    	jae    9271c <_Wfl.fl_vm_exec$a5081fe92876eb37+0x72cc>
   8d270:	48 8b 4c 24 10       	mov    0x10(%rsp),%rcx
   8d275:	48 8b 09             	mov    (%rcx),%rcx
   8d278:	48 69 c0 60 01 00 00 	imul   $0x160,%rax,%rax
   8d27f:	48 8b 4c 01 11       	mov    0x11(%rcx,%rax,1),%rcx
   8d284:	e9 ee 29 00 00       	jmp    8fc77 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4827>
   8d289:	4d 85 c9             	test   %r9,%r9
   8d28c:	7f 14                	jg     8d2a2 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1e52>
   8d28e:	ba 16 00 00 00       	mov    $0x16,%edx
   8d293:	4c 89 f7             	mov    %r14,%rdi
   8d296:	48 8d 35 46 76 f8 ff 	lea    -0x789ba(%rip),%rsi        # 148e3 <__abi_tag+0x145e7>
   8d29d:	e8 5e c1 02 00       	call   b9400 <_Wfl.fl_current_bug$22c9f442c8960c68>
   8d2a2:	49 8b 86 ac 00 00 00 	mov    0xac(%r14),%rax
   8d2a9:	48 ff c8             	dec    %rax
   8d2ac:	0f 80 e5 42 00 00    	jo     91597 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6147>
   8d2b2:	49 89 86 ac 00 00 00 	mov    %rax,0xac(%r14)
   8d2b9:	49 8b 8e a4 00 00 00 	mov    0xa4(%r14),%rcx
   8d2c0:	48 8b 71 08          	mov    0x8(%rcx),%rsi
   8d2c4:	48 39 f0             	cmp    %rsi,%rax
   8d2c7:	0f 83 0a 4a 00 00    	jae    91cd7 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6887>
   8d2cd:	48 8b 11             	mov    (%rcx),%rdx
   8d2d0:	48 8d 3c 40          	lea    (%rax,%rax,2),%rdi
   8d2d4:	48 8b 0c fa          	mov    (%rdx,%rdi,8),%rcx
   8d2d8:	48 8b 44 fa 08       	mov    0x8(%rdx,%rdi,8),%rax
   8d2dd:	f2 0f 10 44 fa 10    	movsd  0x10(%rdx,%rdi,8),%xmm0
   8d2e3:	4d 85 e4             	test   %r12,%r12
   8d2e6:	0f 88 f0 01 00 00    	js     8d4dc <_Wfl.fl_vm_exec$a5081fe92876eb37+0x208c>
   8d2ec:	49 39 f4             	cmp    %rsi,%r12
   8d2ef:	0f 83 92 4a 00 00    	jae    91d87 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6937>
   8d2f5:	4b 8d 34 64          	lea    (%r12,%r12,2),%rsi
   8d2f9:	48 89 0c f2          	mov    %rcx,(%rdx,%rsi,8)
   8d2fd:	48 89 44 f2 08       	mov    %rax,0x8(%rdx,%rsi,8)
   8d302:	f2 0f 11 44 f2 10    	movsd  %xmm0,0x10(%rdx,%rsi,8)
   8d308:	4c 8b 24 24          	mov    (%rsp),%r12
   8d30c:	e9 4a e2 ff ff       	jmp    8b55b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x10b>
   8d311:	41 80 fb 15          	cmp    $0x15,%r11b
   8d315:	0f 87 f3 01 00 00    	ja     8d50e <_Wfl.fl_vm_exec$a5081fe92876eb37+0x20be>
   8d31b:	48 83 7c 24 20 00    	cmpq   $0x0,0x20(%rsp)
   8d321:	7f 14                	jg     8d337 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1ee7>
   8d323:	ba 16 00 00 00       	mov    $0x16,%edx
   8d328:	4c 89 f7             	mov    %r14,%rdi
   8d32b:	48 8d 35 b1 75 f8 ff 	lea    -0x78a4f(%rip),%rsi        # 148e3 <__abi_tag+0x145e7>
   8d332:	e8 c9 c0 02 00       	call   b9400 <_Wfl.fl_current_bug$22c9f442c8960c68>
   8d337:	49 8b 86 ac 00 00 00 	mov    0xac(%r14),%rax
   8d33e:	48 ff c8             	dec    %rax
   8d341:	48 8b 6c 24 08       	mov    0x8(%rsp),%rbp
   8d346:	4c 8b 24 24          	mov    (%rsp),%r12
   8d34a:	48 8b 5c 24 50       	mov    0x50(%rsp),%rbx
   8d34f:	0f 80 9d 42 00 00    	jo     915f2 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x61a2>
   8d355:	49 89 86 ac 00 00 00 	mov    %rax,0xac(%r14)
   8d35c:	49 8b 8e a4 00 00 00 	mov    0xa4(%r14),%rcx
   8d363:	48 3b 41 08          	cmp    0x8(%rcx),%rax
   8d367:	0f 83 17 4b 00 00    	jae    91e84 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6a34>
   8d36d:	48 8b 09             	mov    (%rcx),%rcx
   8d370:	48 8d 14 40          	lea    (%rax,%rax,2),%rdx
   8d374:	f2 0f 10 44 d1 10    	movsd  0x10(%rcx,%rdx,8),%xmm0
   8d37a:	0f 10 0c d1          	movups (%rcx,%rdx,8),%xmm1
   8d37e:	0f 29 8c 24 70 04 00 	movaps %xmm1,0x470(%rsp)
   8d385:	00 
   8d386:	f2 0f 11 84 24 80 04 	movsd  %xmm0,0x480(%rsp)
   8d38d:	00 00 
   8d38f:	48 85 c0             	test   %rax,%rax
   8d392:	7f 1b                	jg     8d3af <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1f5f>
   8d394:	ba 16 00 00 00       	mov    $0x16,%edx
   8d399:	4c 89 f7             	mov    %r14,%rdi
   8d39c:	48 8d 35 40 75 f8 ff 	lea    -0x78ac0(%rip),%rsi        # 148e3 <__abi_tag+0x145e7>
   8d3a3:	e8 58 c0 02 00       	call   b9400 <_Wfl.fl_current_bug$22c9f442c8960c68>
   8d3a8:	49 8b 86 ac 00 00 00 	mov    0xac(%r14),%rax
   8d3af:	48 ff c8             	dec    %rax
   8d3b2:	0f 80 50 42 00 00    	jo     91608 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x61b8>
   8d3b8:	49 89 86 ac 00 00 00 	mov    %rax,0xac(%r14)
   8d3bf:	49 8b 8e a4 00 00 00 	mov    0xa4(%r14),%rcx
   8d3c6:	48 3b 41 08          	cmp    0x8(%rcx),%rax
   8d3ca:	0f 83 9e 4a 00 00    	jae    91e6e <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6a1e>
   8d3d0:	48 8b 09             	mov    (%rcx),%rcx
   8d3d3:	48 8d 04 40          	lea    (%rax,%rax,2),%rax
   8d3d7:	f2 0f 10 44 c1 10    	movsd  0x10(%rcx,%rax,8),%xmm0
   8d3dd:	0f 10 0c c1          	movups (%rcx,%rax,8),%xmm1
   8d3e1:	0f 29 8c 24 50 04 00 	movaps %xmm1,0x450(%rsp)
   8d3e8:	00 
   8d3e9:	f2 0f 11 84 24 60 04 	movsd  %xmm0,0x460(%rsp)
   8d3f0:	00 00 
   8d3f2:	4c 89 f7             	mov    %r14,%rdi
   8d3f5:	48 8d b4 24 50 04 00 	lea    0x450(%rsp),%rsi
   8d3fc:	00 
   8d3fd:	48 8d 94 24 70 04 00 	lea    0x470(%rsp),%rdx
   8d404:	00 
   8d405:	48 89 d9             	mov    %rbx,%rcx
   8d408:	4c 8d 84 24 f8 02 00 	lea    0x2f8(%rsp),%r8
   8d40f:	00 
   8d410:	e8 7b e7 05 00       	call   ebb90 <_Wfl.fl_arith$c1c83b23f6345462>
   8d415:	41 80 be 5c 01 00 00 	cmpb   $0x0,0x15c(%r14)
   8d41c:	00 
   8d41d:	0f 85 38 e1 ff ff    	jne    8b55b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x10b>
   8d423:	49 8b 86 ac 00 00 00 	mov    0xac(%r14),%rax
   8d42a:	48 3d 00 40 00 00    	cmp    $0x4000,%rax
   8d430:	0f 8d 7f fa ff ff    	jge    8ceb5 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1a65>
   8d436:	49 8b 8e a4 00 00 00 	mov    0xa4(%r14),%rcx
   8d43d:	48 3b 41 08          	cmp    0x8(%rcx),%rax
   8d441:	0f 83 3a 4b 00 00    	jae    91f81 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6b31>
   8d447:	48 8b 09             	mov    (%rcx),%rcx
   8d44a:	48 8d 14 40          	lea    (%rax,%rax,2),%rdx
   8d44e:	48 8b b4 24 f8 02 00 	mov    0x2f8(%rsp),%rsi
   8d455:	00 
   8d456:	48 89 34 d1          	mov    %rsi,(%rcx,%rdx,8)
   8d45a:	48 8b b4 24 00 03 00 	mov    0x300(%rsp),%rsi
   8d461:	00 
   8d462:	48 89 74 d1 08       	mov    %rsi,0x8(%rcx,%rdx,8)
   8d467:	f2 0f 10 84 24 08 03 	movsd  0x308(%rsp),%xmm0
   8d46e:	00 00 
   8d470:	f2 0f 11 44 d1 10    	movsd  %xmm0,0x10(%rcx,%rdx,8)
   8d476:	48 ff c0             	inc    %rax
   8d479:	49 89 86 ac 00 00 00 	mov    %rax,0xac(%r14)
   8d480:	e9 d6 e0 ff ff       	jmp    8b55b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x10b>
   8d485:	48 8d 90 09 01 00 00 	lea    0x109(%rax),%rdx
   8d48c:	48 8d 88 01 01 00 00 	lea    0x101(%rax),%rcx
   8d493:	48 05 f9 00 00 00    	add    $0xf9,%rax
   8d499:	48 8b 7c 24 38       	mov    0x38(%rsp),%rdi
   8d49e:	49 81 f9 00 40 00 00 	cmp    $0x4000,%r9
   8d4a5:	4c 8b 24 24          	mov    (%rsp),%r12
   8d4a9:	0f 8d 06 fa ff ff    	jge    8ceb5 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1a65>
   8d4af:	4c 3b 4f 08          	cmp    0x8(%rdi),%r9
   8d4b3:	0f 83 26 49 00 00    	jae    91ddf <_Wfl.fl_vm_exec$a5081fe92876eb37+0x698f>
   8d4b9:	48 8b 12             	mov    (%rdx),%rdx
   8d4bc:	48 8b 09             	mov    (%rcx),%rcx
   8d4bf:	48 8b 00             	mov    (%rax),%rax
   8d4c2:	48 8b 37             	mov    (%rdi),%rsi
   8d4c5:	4b 8d 3c 49          	lea    (%r9,%r9,2),%rdi
   8d4c9:	48 89 04 fe          	mov    %rax,(%rsi,%rdi,8)
   8d4cd:	48 89 4c fe 08       	mov    %rcx,0x8(%rsi,%rdi,8)
   8d4d2:	48 89 54 fe 10       	mov    %rdx,0x10(%rsi,%rdi,8)
   8d4d7:	e9 01 f5 ff ff       	jmp    8c9dd <_Wfl.fl_vm_exec$a5081fe92876eb37+0x158d>
   8d4dc:	49 8b 16             	mov    (%r14),%rdx
   8d4df:	4c 3b 7a 08          	cmp    0x8(%rdx),%r15
   8d4e3:	0f 83 43 49 00 00    	jae    91e2c <_Wfl.fl_vm_exec$a5081fe92876eb37+0x69dc>
   8d4e9:	48 8b 12             	mov    (%rdx),%rdx
   8d4ec:	48 89 8c 1a f9 00 00 	mov    %rcx,0xf9(%rdx,%rbx,1)
   8d4f3:	00 
   8d4f4:	48 89 84 1a 01 01 00 	mov    %rax,0x101(%rdx,%rbx,1)
   8d4fb:	00 
   8d4fc:	f2 0f 11 84 1a 09 01 	movsd  %xmm0,0x109(%rdx,%rbx,1)
   8d503:	00 00 
   8d505:	4c 8b 24 24          	mov    (%rsp),%r12
   8d509:	e9 4d e0 ff ff       	jmp    8b55b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x10b>
   8d50e:	48 8b 44 24 50       	mov    0x50(%rsp),%rax
   8d513:	83 f8 17             	cmp    $0x17,%eax
   8d516:	74 6d                	je     8d585 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x2135>
   8d518:	83 f8 16             	cmp    $0x16,%eax
   8d51b:	0f 85 b5 00 00 00    	jne    8d5d6 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x2186>
   8d521:	48 8b 4c 24 20       	mov    0x20(%rsp),%rcx
   8d526:	48 ff c9             	dec    %rcx
   8d529:	0f 80 bd 41 00 00    	jo     916ec <_Wfl.fl_vm_exec$a5081fe92876eb37+0x629c>
   8d52f:	48 8b 44 24 38       	mov    0x38(%rsp),%rax
   8d534:	48 3b 48 08          	cmp    0x8(%rax),%rcx
   8d538:	48 8b 6c 24 08       	mov    0x8(%rsp),%rbp
   8d53d:	4c 8b 24 24          	mov    (%rsp),%r12
   8d541:	0f 83 d7 49 00 00    	jae    91f1e <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6ace>
   8d547:	48 8b 00             	mov    (%rax),%rax
   8d54a:	48 c1 e1 03          	shl    $0x3,%rcx
   8d54e:	48 8d 0c 49          	lea    (%rcx,%rcx,2),%rcx
   8d552:	48 8b 1c 08          	mov    (%rax,%rcx,1),%rbx
   8d556:	48 83 fb 03          	cmp    $0x3,%rbx
   8d55a:	0f 84 f9 02 00 00    	je     8d859 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x2409>
   8d560:	48 83 fb 02          	cmp    $0x2,%rbx
   8d564:	0f 85 0a 03 00 00    	jne    8d874 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x2424>
   8d56a:	48 c7 04 08 02 00 00 	movq   $0x2,(%rax,%rcx,1)
   8d571:	00 
   8d572:	48 f7 5c 08 08       	negq   0x8(%rax,%rcx,1)
   8d577:	48 c7 44 08 10 00 00 	movq   $0x0,0x10(%rax,%rcx,1)
   8d57e:	00 00 
   8d580:	e9 d6 df ff ff       	jmp    8b55b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x10b>
   8d585:	48 8b 4c 24 20       	mov    0x20(%rsp),%rcx
   8d58a:	48 ff c9             	dec    %rcx
   8d58d:	48 8b 6c 24 08       	mov    0x8(%rsp),%rbp
   8d592:	4c 8b 24 24          	mov    (%rsp),%r12
   8d596:	0f 80 66 41 00 00    	jo     91702 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x62b2>
   8d59c:	48 8b 44 24 38       	mov    0x38(%rsp),%rax
   8d5a1:	48 3b 48 08          	cmp    0x8(%rax),%rcx
   8d5a5:	0f 83 5d 49 00 00    	jae    91f08 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6ab8>
   8d5ab:	48 8b 00             	mov    (%rax),%rax
   8d5ae:	48 c1 e1 03          	shl    $0x3,%rcx
   8d5b2:	48 8d 0c 49          	lea    (%rcx,%rcx,2),%rcx
   8d5b6:	48 8b 34 08          	mov    (%rax,%rcx,1),%rsi
   8d5ba:	48 85 f6             	test   %rsi,%rsi
   8d5bd:	0f 84 76 02 00 00    	je     8d839 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x23e9>
   8d5c3:	48 83 f6 01          	xor    $0x1,%rsi
   8d5c7:	31 d2                	xor    %edx,%edx
   8d5c9:	48 0b 74 08 08       	or     0x8(%rax,%rcx,1),%rsi
   8d5ce:	0f 94 c2             	sete   %dl
   8d5d1:	e9 68 02 00 00       	jmp    8d83e <_Wfl.fl_vm_exec$a5081fe92876eb37+0x23ee>
   8d5d6:	41 8d 43 e8          	lea    -0x18(%r11),%eax
   8d5da:	3c 05                	cmp    $0x5,%al
   8d5dc:	0f 87 5a 01 00 00    	ja     8d73c <_Wfl.fl_vm_exec$a5081fe92876eb37+0x22ec>
   8d5e2:	48 83 7c 24 20 00    	cmpq   $0x0,0x20(%rsp)
   8d5e8:	7f 14                	jg     8d5fe <_Wfl.fl_vm_exec$a5081fe92876eb37+0x21ae>
   8d5ea:	ba 16 00 00 00       	mov    $0x16,%edx
   8d5ef:	4c 89 f7             	mov    %r14,%rdi
   8d5f2:	48 8d 35 ea 72 f8 ff 	lea    -0x78d16(%rip),%rsi        # 148e3 <__abi_tag+0x145e7>
   8d5f9:	e8 02 be 02 00       	call   b9400 <_Wfl.fl_current_bug$22c9f442c8960c68>
   8d5fe:	49 8b 86 ac 00 00 00 	mov    0xac(%r14),%rax
   8d605:	48 ff c8             	dec    %rax
   8d608:	48 8b 6c 24 08       	mov    0x8(%rsp),%rbp
   8d60d:	4c 8b 24 24          	mov    (%rsp),%r12
   8d611:	48 8b 5c 24 50       	mov    0x50(%rsp),%rbx
   8d616:	0f 80 cd 41 00 00    	jo     917e9 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6399>
   8d61c:	49 89 86 ac 00 00 00 	mov    %rax,0xac(%r14)
   8d623:	49 8b 8e a4 00 00 00 	mov    0xa4(%r14),%rcx
   8d62a:	48 3b 41 08          	cmp    0x8(%rcx),%rax
   8d62e:	0f 83 84 49 00 00    	jae    91fb8 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6b68>
   8d634:	48 8b 09             	mov    (%rcx),%rcx
   8d637:	48 8d 14 40          	lea    (%rax,%rax,2),%rdx
   8d63b:	f2 0f 10 44 d1 10    	movsd  0x10(%rcx,%rdx,8),%xmm0
   8d641:	0f 10 0c d1          	movups (%rcx,%rdx,8),%xmm1
   8d645:	0f 29 8c 24 30 04 00 	movaps %xmm1,0x430(%rsp)
   8d64c:	00 
   8d64d:	f2 0f 11 84 24 40 04 	movsd  %xmm0,0x440(%rsp)
   8d654:	00 00 
   8d656:	48 85 c0             	test   %rax,%rax
   8d659:	7f 1b                	jg     8d676 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x2226>
   8d65b:	ba 16 00 00 00       	mov    $0x16,%edx
   8d660:	4c 89 f7             	mov    %r14,%rdi
   8d663:	48 8d 35 79 72 f8 ff 	lea    -0x78d87(%rip),%rsi        # 148e3 <__abi_tag+0x145e7>
   8d66a:	e8 91 bd 02 00       	call   b9400 <_Wfl.fl_current_bug$22c9f442c8960c68>
   8d66f:	49 8b 86 ac 00 00 00 	mov    0xac(%r14),%rax
   8d676:	48 ff c8             	dec    %rax
   8d679:	0f 80 80 41 00 00    	jo     917ff <_Wfl.fl_vm_exec$a5081fe92876eb37+0x63af>
   8d67f:	49 89 86 ac 00 00 00 	mov    %rax,0xac(%r14)
   8d686:	49 8b 8e a4 00 00 00 	mov    0xa4(%r14),%rcx
   8d68d:	48 3b 41 08          	cmp    0x8(%rcx),%rax
   8d691:	0f 83 0b 49 00 00    	jae    91fa2 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6b52>
   8d697:	48 8b 09             	mov    (%rcx),%rcx
   8d69a:	48 8d 04 40          	lea    (%rax,%rax,2),%rax
   8d69e:	f2 0f 10 44 c1 10    	movsd  0x10(%rcx,%rax,8),%xmm0
   8d6a4:	0f 10 0c c1          	movups (%rcx,%rax,8),%xmm1
   8d6a8:	0f 29 8c 24 10 04 00 	movaps %xmm1,0x410(%rsp)
   8d6af:	00 
   8d6b0:	f2 0f 11 84 24 20 04 	movsd  %xmm0,0x420(%rsp)
   8d6b7:	00 00 
   8d6b9:	4c 89 f7             	mov    %r14,%rdi
   8d6bc:	48 8d b4 24 10 04 00 	lea    0x410(%rsp),%rsi
   8d6c3:	00 
   8d6c4:	48 8d 94 24 30 04 00 	lea    0x430(%rsp),%rdx
   8d6cb:	00 
   8d6cc:	48 89 d9             	mov    %rbx,%rcx
   8d6cf:	4c 8d 84 24 e0 02 00 	lea    0x2e0(%rsp),%r8
   8d6d6:	00 
   8d6d7:	e8 84 f9 05 00       	call   ed060 <_Wfl.fl_cmp$e8c241740a9fb998>
   8d6dc:	41 80 be 5c 01 00 00 	cmpb   $0x0,0x15c(%r14)
   8d6e3:	00 
   8d6e4:	0f 85 71 de ff ff    	jne    8b55b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x10b>
   8d6ea:	49 8b 86 ac 00 00 00 	mov    0xac(%r14),%rax
   8d6f1:	48 3d 00 40 00 00    	cmp    $0x4000,%rax
   8d6f7:	0f 8d b8 f7 ff ff    	jge    8ceb5 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1a65>
   8d6fd:	49 8b 8e a4 00 00 00 	mov    0xa4(%r14),%rcx
   8d704:	48 3b 41 08          	cmp    0x8(%rcx),%rax
   8d708:	0f 83 30 4c 00 00    	jae    9233e <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6eee>
   8d70e:	48 8b 09             	mov    (%rcx),%rcx
   8d711:	48 8d 14 40          	lea    (%rax,%rax,2),%rdx
   8d715:	48 8b b4 24 e0 02 00 	mov    0x2e0(%rsp),%rsi
   8d71c:	00 
   8d71d:	48 89 34 d1          	mov    %rsi,(%rcx,%rdx,8)
   8d721:	48 8b b4 24 e8 02 00 	mov    0x2e8(%rsp),%rsi
   8d728:	00 
   8d729:	48 89 74 d1 08       	mov    %rsi,0x8(%rcx,%rdx,8)
   8d72e:	f2 0f 10 84 24 f0 02 	movsd  0x2f0(%rsp),%xmm0
   8d735:	00 00 
   8d737:	e9 34 fd ff ff       	jmp    8d470 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x2020>
   8d73c:	41 8d 43 e2          	lea    -0x1e(%r11),%eax
   8d740:	3c 05                	cmp    $0x5,%al
   8d742:	0f 87 a0 01 00 00    	ja     8d8e8 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x2498>
   8d748:	4d 3b 6f 08          	cmp    0x8(%r15),%r13
   8d74c:	0f 83 4d 49 00 00    	jae    9209f <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6c4f>
   8d752:	49 8b 0f             	mov    (%r15),%rcx
   8d755:	48 8b 74 24 18       	mov    0x18(%rsp),%rsi
   8d75a:	48 8b 44 31 18       	mov    0x18(%rcx,%rsi,1),%rax
   8d75f:	48 89 c2             	mov    %rax,%rdx
   8d762:	48 83 c2 02          	add    $0x2,%rdx
   8d766:	4c 8b 24 24          	mov    (%rsp),%r12
   8d76a:	0f 80 3f 41 00 00    	jo     918af <_Wfl.fl_vm_exec$a5081fe92876eb37+0x645f>
   8d770:	48 89 54 31 18       	mov    %rdx,0x18(%rcx,%rsi,1)
   8d775:	4d 3b 62 08          	cmp    0x8(%r10),%r12
   8d779:	48 8b 54 24 10       	mov    0x10(%rsp),%rdx
   8d77e:	0f 83 b0 47 00 00    	jae    91f34 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6ae4>
   8d784:	49 8b 0a             	mov    (%r10),%rcx
   8d787:	48 8b 4c 11 61       	mov    0x61(%rcx,%rdx,1),%rcx
   8d78c:	48 8b 51 08          	mov    0x8(%rcx),%rdx
   8d790:	48 39 d0             	cmp    %rdx,%rax
   8d793:	0f 83 9b 47 00 00    	jae    91f34 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6ae4>
   8d799:	48 8d 70 01          	lea    0x1(%rax),%rsi
   8d79d:	48 39 d6             	cmp    %rdx,%rsi
   8d7a0:	0f 83 8e 47 00 00    	jae    91f34 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6ae4>
   8d7a6:	48 8b 09             	mov    (%rcx),%rcx
   8d7a9:	0f b7 1c 01          	movzwl (%rcx,%rax,1),%ebx
   8d7ad:	41 80 fb 20          	cmp    $0x20,%r11b
   8d7b1:	0f 95 c0             	setne  %al
   8d7b4:	41 80 fb 21          	cmp    $0x21,%r11b
   8d7b8:	40 0f 94 c5          	sete   %bpl
   8d7bc:	40 30 e8             	xor    %bpl,%al
   8d7bf:	b1 01                	mov    $0x1,%cl
   8d7c1:	0f 85 65 03 00 00    	jne    8db2c <_Wfl.fl_vm_exec$a5081fe92876eb37+0x26dc>
   8d7c7:	48 83 7c 24 20 00    	cmpq   $0x0,0x20(%rsp)
   8d7cd:	7f 19                	jg     8d7e8 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x2398>
   8d7cf:	ba 16 00 00 00       	mov    $0x16,%edx
   8d7d4:	4c 89 f7             	mov    %r14,%rdi
   8d7d7:	48 8d 35 05 71 f8 ff 	lea    -0x78efb(%rip),%rsi        # 148e3 <__abi_tag+0x145e7>
   8d7de:	e8 1d bc 02 00       	call   b9400 <_Wfl.fl_current_bug$22c9f442c8960c68>
   8d7e3:	4c 8b 5c 24 28       	mov    0x28(%rsp),%r11
   8d7e8:	49 8b 86 ac 00 00 00 	mov    0xac(%r14),%rax
   8d7ef:	48 ff c8             	dec    %rax
   8d7f2:	0f 80 e0 41 00 00    	jo     919d8 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6588>
   8d7f8:	49 89 86 ac 00 00 00 	mov    %rax,0xac(%r14)
   8d7ff:	49 8b 8e a4 00 00 00 	mov    0xa4(%r14),%rcx
   8d806:	48 3b 41 08          	cmp    0x8(%rcx),%rax
   8d80a:	0f 83 a1 4d 00 00    	jae    925b1 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x7161>
   8d810:	48 8b 09             	mov    (%rcx),%rcx
   8d813:	48 c1 e0 03          	shl    $0x3,%rax
   8d817:	48 8d 04 40          	lea    (%rax,%rax,2),%rax
   8d81b:	48 8b 14 01          	mov    (%rcx,%rax,1),%rdx
   8d81f:	48 85 d2             	test   %rdx,%rdx
   8d822:	0f 84 fc 02 00 00    	je     8db24 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x26d4>
   8d828:	48 83 f2 01          	xor    $0x1,%rdx
   8d82c:	48 0b 54 01 08       	or     0x8(%rcx,%rax,1),%rdx
   8d831:	0f 95 c0             	setne  %al
   8d834:	e9 ed 02 00 00       	jmp    8db26 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x26d6>
   8d839:	ba 01 00 00 00       	mov    $0x1,%edx
   8d83e:	48 c7 04 08 01 00 00 	movq   $0x1,(%rax,%rcx,1)
   8d845:	00 
   8d846:	48 89 54 08 08       	mov    %rdx,0x8(%rax,%rcx,1)
   8d84b:	48 c7 44 08 10 00 00 	movq   $0x0,0x10(%rax,%rcx,1)
   8d852:	00 00 
   8d854:	e9 02 dd ff ff       	jmp    8b55b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x10b>
   8d859:	48 c7 04 08 03 00 00 	movq   $0x3,(%rax,%rcx,1)
   8d860:	00 
   8d861:	48 c7 44 08 08 00 00 	movq   $0x0,0x8(%rax,%rcx,1)
   8d868:	00 00 
   8d86a:	80 74 08 17 80       	xorb   $0x80,0x17(%rax,%rcx,1)
   8d86f:	e9 e7 dc ff ff       	jmp    8b55b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x10b>
   8d874:	e8 f7 b6 0c 00       	call   158f70 <__wolf_rt_strbuf_new>
   8d879:	49 89 c7             	mov    %rax,%r15
   8d87c:	ba 0e 00 00 00       	mov    $0xe,%edx
   8d881:	48 89 c7             	mov    %rax,%rdi
   8d884:	48 8d 35 da 70 f8 ff 	lea    -0x78f26(%rip),%rsi        # 14965 <__abi_tag+0x14669>
   8d88b:	31 c9                	xor    %ecx,%ecx
   8d88d:	e8 8e b7 0c 00       	call   159020 <__wolf_rt_strbuf_str>
   8d892:	48 89 df             	mov    %rbx,%rdi
   8d895:	e8 06 e2 06 00       	call   fbaa0 <_Wfl.fl_type_name$425de879c2cc2a41>
   8d89a:	4c 89 ff             	mov    %r15,%rdi
   8d89d:	48 89 c6             	mov    %rax,%rsi
   8d8a0:	31 c9                	xor    %ecx,%ecx
   8d8a2:	e8 79 b7 0c 00       	call   159020 <__wolf_rt_strbuf_str>
   8d8a7:	4c 89 ff             	mov    %r15,%rdi
   8d8aa:	48 8d b4 24 00 02 00 	lea    0x200(%rsp),%rsi
   8d8b1:	00 
   8d8b2:	e8 69 b3 0c 00       	call   158c20 <__wolf_rt_strbuf_finish>
   8d8b7:	48 8b 8c 24 00 02 00 	mov    0x200(%rsp),%rcx
   8d8be:	00 
   8d8bf:	4c 8b 84 24 08 02 00 	mov    0x208(%rsp),%r8
   8d8c6:	00 
   8d8c7:	ba 04 00 00 00       	mov    $0x4,%edx
   8d8cc:	4c 89 f7             	mov    %r14,%rdi
   8d8cf:	48 8d 35 1a 0b f9 ff 	lea    -0x6f4e6(%rip),%rsi        # 1e3f0 <__abi_tag+0x1e0f4>
   8d8d6:	4c 8d 8c 24 a8 1f 00 	lea    0x1fa8(%rsp),%r9
   8d8dd:	00 
   8d8de:	e8 ed 51 08 00       	call   112ad0 <_Wfl.fl_raise_bytes.bytesview.4$e8b5fa06fc8adc2d>
   8d8e3:	e9 73 dc ff ff       	jmp    8b55b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x10b>
   8d8e8:	41 80 fb 24          	cmp    $0x24,%r11b
   8d8ec:	0f 85 46 01 00 00    	jne    8da38 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x25e8>
   8d8f2:	4d 3b 6f 08          	cmp    0x8(%r15),%r13
   8d8f6:	4c 8b 24 24          	mov    (%rsp),%r12
   8d8fa:	0f 83 4d 4f 00 00    	jae    9284d <_Wfl.fl_vm_exec$a5081fe92876eb37+0x73fd>
   8d900:	49 8b 0f             	mov    (%r15),%rcx
   8d903:	48 8b 74 24 18       	mov    0x18(%rsp),%rsi
   8d908:	48 8b 44 31 18       	mov    0x18(%rcx,%rsi,1),%rax
   8d90d:	48 89 c2             	mov    %rax,%rdx
   8d910:	48 ff c2             	inc    %rdx
   8d913:	48 8b 6c 24 08       	mov    0x8(%rsp),%rbp
   8d918:	4c 8b 6c 24 20       	mov    0x20(%rsp),%r13
   8d91d:	0f 80 cb 40 00 00    	jo     919ee <_Wfl.fl_vm_exec$a5081fe92876eb37+0x659e>
   8d923:	48 89 54 31 18       	mov    %rdx,0x18(%rcx,%rsi,1)
   8d928:	4d 3b 62 08          	cmp    0x8(%r10),%r12
   8d92c:	48 8b 54 24 10       	mov    0x10(%rsp),%rdx
   8d931:	48 8b 74 24 38       	mov    0x38(%rsp),%rsi
   8d936:	0f 83 8f 47 00 00    	jae    920cb <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6c7b>
   8d93c:	49 8b 0a             	mov    (%r10),%rcx
   8d93f:	48 8b 4c 11 61       	mov    0x61(%rcx,%rdx,1),%rcx
   8d944:	48 3b 41 08          	cmp    0x8(%rcx),%rax
   8d948:	0f 83 7d 47 00 00    	jae    920cb <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6c7b>
   8d94e:	48 8b 09             	mov    (%rcx),%rcx
   8d951:	0f b6 0c 01          	movzbl (%rcx,%rax,1),%ecx
   8d955:	49 29 cd             	sub    %rcx,%r13
   8d958:	0f 80 67 3f 00 00    	jo     918c5 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6475>
   8d95e:	49 ff cd             	dec    %r13
   8d961:	0f 80 5e 3f 00 00    	jo     918c5 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6475>
   8d967:	4c 3b 6e 08          	cmp    0x8(%rsi),%r13
   8d96b:	0f 83 4e 4b 00 00    	jae    924bf <_Wfl.fl_vm_exec$a5081fe92876eb37+0x706f>
   8d971:	48 8b 06             	mov    (%rsi),%rax
   8d974:	4a 8d 14 ed 00 00 00 	lea    0x0(,%r13,8),%rdx
   8d97b:	00 
   8d97c:	4c 8d 3c 52          	lea    (%rdx,%rdx,2),%r15
   8d980:	f2 42 0f 10 44 38 10 	movsd  0x10(%rax,%r15,1),%xmm0
   8d987:	42 0f 10 0c 38       	movups (%rax,%r15,1),%xmm1
   8d98c:	4a 8b 04 38          	mov    (%rax,%r15,1),%rax
   8d990:	0f 29 8c 24 50 02 00 	movaps %xmm1,0x250(%rsp)
   8d997:	00 
   8d998:	f2 0f 11 84 24 60 02 	movsd  %xmm0,0x260(%rsp)
   8d99f:	00 00 
   8d9a1:	48 83 f8 09          	cmp    $0x9,%rax
   8d9a5:	0f 85 56 01 00 00    	jne    8db01 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x26b1>
   8d9ab:	49 8d 5d 01          	lea    0x1(%r13),%rbx
   8d9af:	4c 89 f7             	mov    %r14,%rdi
   8d9b2:	48 8d b4 24 50 02 00 	lea    0x250(%rsp),%rsi
   8d9b9:	00 
   8d9ba:	48 89 da             	mov    %rbx,%rdx
   8d9bd:	45 31 c0             	xor    %r8d,%r8d
   8d9c0:	4c 8d 8c 24 c8 02 00 	lea    0x2c8(%rsp),%r9
   8d9c7:	00 
   8d9c8:	e8 33 f9 04 00       	call   dd300 <_Wfl.fl_native_invoke$a020102912b2bd4e>
   8d9cd:	41 80 be 5c 01 00 00 	cmpb   $0x0,0x15c(%r14)
   8d9d4:	00 
   8d9d5:	0f 85 80 db ff ff    	jne    8b55b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x10b>
   8d9db:	4d 89 ae ac 00 00 00 	mov    %r13,0xac(%r14)
   8d9e2:	49 81 fd 00 40 00 00 	cmp    $0x4000,%r13
   8d9e9:	0f 8d c6 f4 ff ff    	jge    8ceb5 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1a65>
   8d9ef:	49 8b 86 a4 00 00 00 	mov    0xa4(%r14),%rax
   8d9f6:	4c 3b 68 08          	cmp    0x8(%rax),%r13
   8d9fa:	0f 83 80 49 00 00    	jae    92380 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6f30>
   8da00:	48 8b 00             	mov    (%rax),%rax
   8da03:	48 8b 8c 24 c8 02 00 	mov    0x2c8(%rsp),%rcx
   8da0a:	00 
   8da0b:	4a 89 0c 38          	mov    %rcx,(%rax,%r15,1)
   8da0f:	48 8b 8c 24 d0 02 00 	mov    0x2d0(%rsp),%rcx
   8da16:	00 
   8da17:	4a 89 4c 38 08       	mov    %rcx,0x8(%rax,%r15,1)
   8da1c:	f2 0f 10 84 24 d8 02 	movsd  0x2d8(%rsp),%xmm0
   8da23:	00 00 
   8da25:	f2 42 0f 11 44 38 10 	movsd  %xmm0,0x10(%rax,%r15,1)
   8da2c:	49 89 9e ac 00 00 00 	mov    %rbx,0xac(%r14)
   8da33:	e9 df 14 00 00       	jmp    8ef17 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x3ac7>
   8da38:	48 8b 44 24 50       	mov    0x50(%rsp),%rax
   8da3d:	83 c0 db             	add    $0xffffffdb,%eax
   8da40:	83 f8 13             	cmp    $0x13,%eax
   8da43:	0f 87 af 03 00 00    	ja     8ddf8 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x29a8>
   8da49:	48 8d 0d ac 66 f8 ff 	lea    -0x79954(%rip),%rcx        # 140fc <__abi_tag+0x13e00>
   8da50:	48 63 04 81          	movslq (%rcx,%rax,4),%rax
   8da54:	48 01 c8             	add    %rcx,%rax
   8da57:	ff e0                	jmp    *%rax
   8da59:	49 83 7f 08 00       	cmpq   $0x0,0x8(%r15)
   8da5e:	48 8b 0c 24          	mov    (%rsp),%rcx
   8da62:	75 31                	jne    8da95 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x2645>
   8da64:	ba 0c 00 00 00       	mov    $0xc,%edx
   8da69:	4c 89 f7             	mov    %r14,%rdi
   8da6c:	48 8d 35 cd 6d f8 ff 	lea    -0x79233(%rip),%rsi        # 14840 <__abi_tag+0x14544>
   8da73:	4c 8b 44 24 70       	mov    0x70(%rsp),%r8
   8da78:	4c 8b 4c 24 50       	mov    0x50(%rsp),%r9
   8da7d:	6a 20                	push   $0x20
   8da7f:	48 8d 05 92 38 f9 ff 	lea    -0x6c76e(%rip),%rax        # 21318 <__abi_tag+0x2101c>
   8da86:	50                   	push   %rax
   8da87:	e8 d4 f1 06 00       	call   fcc60 <_Wfl.fl_vm_bug_at$494622488d4f7894>
   8da8c:	4c 8b 5c 24 38       	mov    0x38(%rsp),%r11
   8da91:	48 83 c4 10          	add    $0x10,%rsp
   8da95:	41 80 fb 25          	cmp    $0x25,%r11b
   8da99:	0f 85 32 01 00 00    	jne    8dbd1 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x2781>
   8da9f:	49 8b 86 ac 00 00 00 	mov    0xac(%r14),%rax
   8daa6:	48 85 c0             	test   %rax,%rax
   8daa9:	7f 1b                	jg     8dac6 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x2676>
   8daab:	ba 16 00 00 00       	mov    $0x16,%edx
   8dab0:	4c 89 f7             	mov    %r14,%rdi
   8dab3:	48 8d 35 29 6e f8 ff 	lea    -0x791d7(%rip),%rsi        # 148e3 <__abi_tag+0x145e7>
   8daba:	e8 41 b9 02 00       	call   b9400 <_Wfl.fl_current_bug$22c9f442c8960c68>
   8dabf:	49 8b 86 ac 00 00 00 	mov    0xac(%r14),%rax
   8dac6:	48 ff c8             	dec    %rax
   8dac9:	0f 80 e7 41 00 00    	jo     91cb6 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6866>
   8dacf:	49 89 86 ac 00 00 00 	mov    %rax,0xac(%r14)
   8dad6:	49 8b 8e a4 00 00 00 	mov    0xa4(%r14),%rcx
   8dadd:	48 3b 41 08          	cmp    0x8(%rcx),%rax
   8dae1:	0f 83 65 47 00 00    	jae    9224c <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6dfc>
   8dae7:	48 8b 09             	mov    (%rcx),%rcx
   8daea:	48 8d 04 40          	lea    (%rax,%rax,2),%rax
   8daee:	0f 10 04 c1          	movups (%rcx,%rax,8),%xmm0
   8daf2:	0f 29 44 24 50       	movaps %xmm0,0x50(%rsp)
   8daf7:	48 8b 5c c1 10       	mov    0x10(%rcx,%rax,8),%rbx
   8dafc:	e9 da 00 00 00       	jmp    8dbdb <_Wfl.fl_vm_exec$a5081fe92876eb37+0x278b>
   8db01:	4c 89 f7             	mov    %r14,%rdi
   8db04:	48 8d b4 24 50 02 00 	lea    0x250(%rsp),%rsi
   8db0b:	00 
   8db0c:	48 89 ca             	mov    %rcx,%rdx
   8db0f:	4c 89 e9             	mov    %r13,%rcx
   8db12:	4c 8b 44 24 70       	mov    0x70(%rsp),%r8
   8db17:	45 31 c9             	xor    %r9d,%r9d
   8db1a:	e8 21 9e 02 00       	call   b7940 <_Wfl.fl_frame_push$10db4ef30ec77391>
   8db1f:	e9 37 da ff ff       	jmp    8b55b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x10b>
   8db24:	31 c0                	xor    %eax,%eax
   8db26:	40 38 e8             	cmp    %bpl,%al
   8db29:	0f 94 c1             	sete   %cl
   8db2c:	41 80 fb 22          	cmp    $0x22,%r11b
   8db30:	0f 94 c0             	sete   %al
   8db33:	44 89 da             	mov    %r11d,%edx
   8db36:	80 e2 3e             	and    $0x3e,%dl
   8db39:	80 fa 22             	cmp    $0x22,%dl
   8db3c:	48 8b 6c 24 08       	mov    0x8(%rsp),%rbp
   8db41:	75 4a                	jne    8db8d <_Wfl.fl_vm_exec$a5081fe92876eb37+0x273d>
   8db43:	49 8b 8e ac 00 00 00 	mov    0xac(%r14),%rcx
   8db4a:	48 ff c9             	dec    %rcx
   8db4d:	0f 80 b1 3e 00 00    	jo     91a04 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x65b4>
   8db53:	49 8b 96 a4 00 00 00 	mov    0xa4(%r14),%rdx
   8db5a:	48 3b 4a 08          	cmp    0x8(%rdx),%rcx
   8db5e:	0f 83 e7 4a 00 00    	jae    9264b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x71fb>
   8db64:	48 8b 12             	mov    (%rdx),%rdx
   8db67:	48 c1 e1 03          	shl    $0x3,%rcx
   8db6b:	48 8d 0c 49          	lea    (%rcx,%rcx,2),%rcx
   8db6f:	48 8b 34 0a          	mov    (%rdx,%rcx,1),%rsi
   8db73:	48 85 f6             	test   %rsi,%rsi
   8db76:	74 0e                	je     8db86 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x2736>
   8db78:	48 83 f6 01          	xor    $0x1,%rsi
   8db7c:	48 0b 74 0a 08       	or     0x8(%rdx,%rcx,1),%rsi
   8db81:	0f 95 c1             	setne  %cl
   8db84:	eb 02                	jmp    8db88 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x2738>
   8db86:	31 c9                	xor    %ecx,%ecx
   8db88:	38 c1                	cmp    %al,%cl
   8db8a:	0f 94 c1             	sete   %cl
   8db8d:	84 c9                	test   %cl,%cl
   8db8f:	0f 84 c6 d9 ff ff    	je     8b55b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x10b>
   8db95:	49 8b 86 b4 00 00 00 	mov    0xb4(%r14),%rax
   8db9c:	4c 3b 68 08          	cmp    0x8(%rax),%r13
   8dba0:	0f 83 9e 45 00 00    	jae    92144 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6cf4>
   8dba6:	48 89 d9             	mov    %rbx,%rcx
   8dba9:	48 f7 d9             	neg    %rcx
   8dbac:	41 80 fb 1f          	cmp    $0x1f,%r11b
   8dbb0:	48 0f 45 cb          	cmovne %rbx,%rcx
   8dbb4:	48 8b 00             	mov    (%rax),%rax
   8dbb7:	48 8b 54 24 18       	mov    0x18(%rsp),%rdx
   8dbbc:	48 03 4c 10 18       	add    0x18(%rax,%rdx,1),%rcx
   8dbc1:	0f 80 f0 3d 00 00    	jo     919b7 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6567>
   8dbc7:	48 89 4c 10 18       	mov    %rcx,0x18(%rax,%rdx,1)
   8dbcc:	e9 8a d9 ff ff       	jmp    8b55b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x10b>
   8dbd1:	0f 57 c0             	xorps  %xmm0,%xmm0
   8dbd4:	0f 29 44 24 50       	movaps %xmm0,0x50(%rsp)
   8dbd9:	31 db                	xor    %ebx,%ebx
   8dbdb:	48 8b 6c 24 08       	mov    0x8(%rsp),%rbp
   8dbe0:	48 8b 4c 24 18       	mov    0x18(%rsp),%rcx
   8dbe5:	49 8b 86 b4 00 00 00 	mov    0xb4(%r14),%rax
   8dbec:	4c 3b 68 08          	cmp    0x8(%rax),%r13
   8dbf0:	0f 83 b9 46 00 00    	jae    922af <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6e5f>
   8dbf6:	48 8b 00             	mov    (%rax),%rax
   8dbf9:	48 8b 44 08 38       	mov    0x38(%rax,%rcx,1),%rax
   8dbfe:	49 89 86 b5 01 00 00 	mov    %rax,0x1b5(%r14)
   8dc05:	4c 89 f7             	mov    %r14,%rdi
   8dc08:	4c 8b 7c 24 68       	mov    0x68(%rsp),%r15
   8dc0d:	4c 89 fe             	mov    %r15,%rsi
   8dc10:	e8 8b 9a 02 00       	call   b76a0 <_Wfl.fl_close_upvals$345768e8f0248c70>
   8dc15:	4c 89 f7             	mov    %r14,%rdi
   8dc18:	48 8d b4 24 08 20 00 	lea    0x2008(%rsp),%rsi
   8dc1f:	00 
   8dc20:	e8 0b 9a 02 00       	call   b7630 <_Wfl.fl_return_frame$f89d02dfe330bb27>
   8dc25:	49 8b 86 b4 00 00 00 	mov    0xb4(%r14),%rax
   8dc2c:	48 39 68 08          	cmp    %rbp,0x8(%rax)
   8dc30:	0f 84 a3 39 00 00    	je     915d9 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6189>
   8dc36:	4d 89 be ac 00 00 00 	mov    %r15,0xac(%r14)
   8dc3d:	49 81 ff 00 40 00 00 	cmp    $0x4000,%r15
   8dc44:	4c 89 7c 24 68       	mov    %r15,0x68(%rsp)
   8dc49:	7c 11                	jl     8dc5c <_Wfl.fl_vm_exec$a5081fe92876eb37+0x280c>
   8dc4b:	4c 89 f7             	mov    %r14,%rdi
   8dc4e:	e8 0d 99 02 00       	call   b7560 <_Wfl.fl_push_overflow$ec8cb5daf6580c79>
   8dc53:	4c 8b 24 24          	mov    (%rsp),%r12
   8dc57:	e9 ff d8 ff ff       	jmp    8b55b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x10b>
   8dc5c:	49 8b 86 a4 00 00 00 	mov    0xa4(%r14),%rax
   8dc63:	4c 3b 78 08          	cmp    0x8(%rax),%r15
   8dc67:	0f 83 e8 4c 00 00    	jae    92955 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x7505>
   8dc6d:	48 8b 00             	mov    (%rax),%rax
   8dc70:	4b 8d 0c 7f          	lea    (%r15,%r15,2),%rcx
   8dc74:	0f 28 44 24 50       	movaps 0x50(%rsp),%xmm0
   8dc79:	0f 11 04 c8          	movups %xmm0,(%rax,%rcx,8)
   8dc7d:	48 89 5c c8 10       	mov    %rbx,0x10(%rax,%rcx,8)
   8dc82:	49 8d 47 01          	lea    0x1(%r15),%rax
   8dc86:	49 89 86 ac 00 00 00 	mov    %rax,0xac(%r14)
   8dc8d:	4c 8b 24 24          	mov    (%rsp),%r12
   8dc91:	e9 c5 d8 ff ff       	jmp    8b55b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x10b>
   8dc96:	4d 3b 6f 08          	cmp    0x8(%r15),%r13
   8dc9a:	0f 83 b4 46 00 00    	jae    92354 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6f04>
   8dca0:	49 8b 0f             	mov    (%r15),%rcx
   8dca3:	48 8b 74 24 18       	mov    0x18(%rsp),%rsi
   8dca8:	48 8b 44 31 18       	mov    0x18(%rcx,%rsi,1),%rax
   8dcad:	48 89 c2             	mov    %rax,%rdx
   8dcb0:	48 83 c2 02          	add    $0x2,%rdx
   8dcb4:	0f 80 80 40 00 00    	jo     91d3a <_Wfl.fl_vm_exec$a5081fe92876eb37+0x68ea>
   8dcba:	48 89 54 31 18       	mov    %rdx,0x18(%rcx,%rsi,1)
   8dcbf:	48 8b 0c 24          	mov    (%rsp),%rcx
   8dcc3:	49 3b 4a 08          	cmp    0x8(%r10),%rcx
   8dcc7:	0f 83 2c 47 00 00    	jae    923f9 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6fa9>
   8dccd:	49 8b 0a             	mov    (%r10),%rcx
   8dcd0:	48 8b 54 24 10       	mov    0x10(%rsp),%rdx
   8dcd5:	48 8b 4c 11 61       	mov    0x61(%rcx,%rdx,1),%rcx
   8dcda:	48 8b 51 08          	mov    0x8(%rcx),%rdx
   8dcde:	48 39 d0             	cmp    %rdx,%rax
   8dce1:	0f 83 12 47 00 00    	jae    923f9 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6fa9>
   8dce7:	48 8d 70 01          	lea    0x1(%rax),%rsi
   8dceb:	48 39 d6             	cmp    %rdx,%rsi
   8dcee:	0f 83 05 47 00 00    	jae    923f9 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6fa9>
   8dcf4:	48 8b 09             	mov    (%rcx),%rcx
   8dcf7:	0f b6 1c 01          	movzbl (%rcx,%rax,1),%ebx
   8dcfb:	44 0f b6 6c 01 01    	movzbl 0x1(%rcx,%rax,1),%r13d
   8dd01:	45 89 ec             	mov    %r13d,%r12d
   8dd04:	41 c1 e4 08          	shl    $0x8,%r12d
   8dd08:	be 06 00 00 00       	mov    $0x6,%esi
   8dd0d:	ba 38 00 00 00       	mov    $0x38,%edx
   8dd12:	4c 89 f7             	mov    %r14,%rdi
   8dd15:	48 8d 8c 24 80 01 00 	lea    0x180(%rsp),%rcx
   8dd1c:	00 
   8dd1d:	e8 8e 6d ff ff       	call   84ab0 <_Wfl.fl_gc_alloc$34adf57915ed2b38>
   8dd22:	49 09 dc             	or     %rbx,%r12
   8dd25:	0f 84 ee 07 00 00    	je     8e519 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x30c9>
   8dd2b:	4f 8d 3c 24          	lea    (%r12,%r12,1),%r15
   8dd2f:	41 c1 e5 09          	shl    $0x9,%r13d
   8dd33:	45 8d 6c 5d 00       	lea    0x0(%r13,%rbx,2),%r13d
   8dd38:	31 ed                	xor    %ebp,%ebp
   8dd3a:	49 8b 86 ac 00 00 00 	mov    0xac(%r14),%rax
   8dd41:	4c 29 f8             	sub    %r15,%rax
   8dd44:	0f 80 73 3a 00 00    	jo     917bd <_Wfl.fl_vm_exec$a5081fe92876eb37+0x636d>
   8dd4a:	48 01 e8             	add    %rbp,%rax
   8dd4d:	0f 80 6a 3a 00 00    	jo     917bd <_Wfl.fl_vm_exec$a5081fe92876eb37+0x636d>
   8dd53:	49 8b 8e a4 00 00 00 	mov    0xa4(%r14),%rcx
   8dd5a:	48 8b 51 08          	mov    0x8(%rcx),%rdx
   8dd5e:	48 39 d0             	cmp    %rdx,%rax
   8dd61:	0f 83 cf 44 00 00    	jae    92236 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6de6>
   8dd67:	48 8b 09             	mov    (%rcx),%rcx
   8dd6a:	48 8d 34 40          	lea    (%rax,%rax,2),%rsi
   8dd6e:	f2 0f 10 44 f1 10    	movsd  0x10(%rcx,%rsi,8),%xmm0
   8dd74:	0f 10 0c f1          	movups (%rcx,%rsi,8),%xmm1
   8dd78:	48 8b 1c f1          	mov    (%rcx,%rsi,8),%rbx
   8dd7c:	0f 29 8c 24 d0 03 00 	movaps %xmm1,0x3d0(%rsp)
   8dd83:	00 
   8dd84:	f2 0f 11 84 24 e0 03 	movsd  %xmm0,0x3e0(%rsp)
   8dd8b:	00 00 
   8dd8d:	48 ff c0             	inc    %rax
   8dd90:	0f 80 00 3c 00 00    	jo     91996 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6546>
   8dd96:	48 39 d0             	cmp    %rdx,%rax
   8dd99:	0f 83 42 43 00 00    	jae    920e1 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6c91>
   8dd9f:	48 8d 04 40          	lea    (%rax,%rax,2),%rax
   8dda3:	f2 0f 10 44 c1 10    	movsd  0x10(%rcx,%rax,8),%xmm0
   8dda9:	0f 10 0c c1          	movups (%rcx,%rax,8),%xmm1
   8ddad:	0f 29 8c 24 b0 03 00 	movaps %xmm1,0x3b0(%rsp)
   8ddb4:	00 
   8ddb5:	f2 0f 11 84 24 c0 03 	movsd  %xmm0,0x3c0(%rsp)
   8ddbc:	00 00 
   8ddbe:	4c 89 f7             	mov    %r14,%rdi
   8ddc1:	48 8d b4 24 80 01 00 	lea    0x180(%rsp),%rsi
   8ddc8:	00 
   8ddc9:	48 8d 94 24 d0 03 00 	lea    0x3d0(%rsp),%rdx
   8ddd0:	00 
   8ddd1:	48 8d 8c 24 b0 03 00 	lea    0x3b0(%rsp),%rcx
   8ddd8:	00 
   8ddd9:	e8 d2 17 03 00       	call   bf5b0 <_Wfl.fl_map_set$540fcc92758450a0>
   8ddde:	84 c0                	test   %al,%al
   8dde0:	0f 84 c4 06 00 00    	je     8e4aa <_Wfl.fl_vm_exec$a5081fe92876eb37+0x305a>
   8dde6:	48 83 c5 02          	add    $0x2,%rbp
   8ddea:	49 39 ed             	cmp    %rbp,%r13
   8dded:	0f 85 47 ff ff ff    	jne    8dd3a <_Wfl.fl_vm_exec$a5081fe92876eb37+0x28ea>
   8ddf3:	e9 21 07 00 00       	jmp    8e519 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x30c9>
   8ddf8:	44 89 d8             	mov    %r11d,%eax
   8ddfb:	24 fe                	and    $0xfe,%al
   8ddfd:	3c 2a                	cmp    $0x2a,%al
   8ddff:	0f 85 9f 05 00 00    	jne    8e3a4 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x2f54>
   8de05:	80 7c 24 28 2b       	cmpb   $0x2b,0x28(%rsp)
   8de0a:	0f 85 4c 07 00 00    	jne    8e55c <_Wfl.fl_vm_exec$a5081fe92876eb37+0x310c>
   8de10:	48 83 7c 24 20 00    	cmpq   $0x0,0x20(%rsp)
   8de16:	7f 14                	jg     8de2c <_Wfl.fl_vm_exec$a5081fe92876eb37+0x29dc>
   8de18:	ba 16 00 00 00       	mov    $0x16,%edx
   8de1d:	4c 89 f7             	mov    %r14,%rdi
   8de20:	48 8d 35 bc 6a f8 ff 	lea    -0x79544(%rip),%rsi        # 148e3 <__abi_tag+0x145e7>
   8de27:	e8 d4 b5 02 00       	call   b9400 <_Wfl.fl_current_bug$22c9f442c8960c68>
   8de2c:	49 8b 86 ac 00 00 00 	mov    0xac(%r14),%rax
   8de33:	48 ff c8             	dec    %rax
   8de36:	0f 80 5e 40 00 00    	jo     91e9a <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6a4a>
   8de3c:	49 89 86 ac 00 00 00 	mov    %rax,0xac(%r14)
   8de43:	49 8b 8e a4 00 00 00 	mov    0xa4(%r14),%rcx
   8de4a:	48 3b 41 08          	cmp    0x8(%rcx),%rax
   8de4e:	0f 83 2e 48 00 00    	jae    92682 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x7232>
   8de54:	48 8b 09             	mov    (%rcx),%rcx
   8de57:	48 8d 14 40          	lea    (%rax,%rax,2),%rdx
   8de5b:	48 8b 74 d1 10       	mov    0x10(%rcx,%rdx,8),%rsi
   8de60:	0f 10 04 d1          	movups (%rcx,%rdx,8),%xmm0
   8de64:	0f 29 84 24 e0 00 00 	movaps %xmm0,0xe0(%rsp)
   8de6b:	00 
   8de6c:	48 89 b4 24 f0 00 00 	mov    %rsi,0xf0(%rsp)
   8de73:	00 
   8de74:	e9 01 07 00 00       	jmp    8e57a <_Wfl.fl_vm_exec$a5081fe92876eb37+0x312a>
   8de79:	4c 89 ac 24 80 00 00 	mov    %r13,0x80(%rsp)
   8de80:	00 
   8de81:	4d 3b 6f 08          	cmp    0x8(%r15),%r13
   8de85:	0f 83 fd 45 00 00    	jae    92488 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x7038>
   8de8b:	49 8b 07             	mov    (%r15),%rax
   8de8e:	48 8b 74 24 18       	mov    0x18(%rsp),%rsi
   8de93:	48 8b 54 30 18       	mov    0x18(%rax,%rsi,1),%rdx
   8de98:	48 89 d1             	mov    %rdx,%rcx
   8de9b:	48 83 c1 02          	add    $0x2,%rcx
   8de9f:	0f 80 69 3e 00 00    	jo     91d0e <_Wfl.fl_vm_exec$a5081fe92876eb37+0x68be>
   8dea5:	48 89 4c 30 18       	mov    %rcx,0x18(%rax,%rsi,1)
   8deaa:	49 8b 42 08          	mov    0x8(%r10),%rax
   8deae:	48 39 04 24          	cmp    %rax,(%rsp)
   8deb2:	0f 83 b2 44 00 00    	jae    9236a <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6f1a>
   8deb8:	49 8b 0a             	mov    (%r10),%rcx
   8debb:	48 8b 74 24 10       	mov    0x10(%rsp),%rsi
   8dec0:	48 8b 74 31 61       	mov    0x61(%rcx,%rsi,1),%rsi
   8dec5:	48 8b 7e 08          	mov    0x8(%rsi),%rdi
   8dec9:	48 39 fa             	cmp    %rdi,%rdx
   8decc:	0f 83 98 44 00 00    	jae    9236a <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6f1a>
   8ded2:	4c 8d 42 01          	lea    0x1(%rdx),%r8
   8ded6:	49 39 f8             	cmp    %rdi,%r8
   8ded9:	0f 83 8b 44 00 00    	jae    9236a <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6f1a>
   8dedf:	48 8b 7c 24 10       	mov    0x10(%rsp),%rdi
   8dee4:	48 8b 7c 39 29       	mov    0x29(%rcx,%rdi,1),%rdi
   8dee9:	48 8b 36             	mov    (%rsi),%rsi
   8deec:	0f b7 14 16          	movzwl (%rsi,%rdx,1),%edx
   8def0:	48 3b 57 08          	cmp    0x8(%rdi),%rdx
   8def4:	0f 83 15 45 00 00    	jae    9240f <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6fbf>
   8defa:	48 8b 3f             	mov    (%rdi),%rdi
   8defd:	4c 8d 04 52          	lea    (%rdx,%rdx,2),%r8
   8df01:	4a 8b 34 c7          	mov    (%rdi,%r8,8),%rsi
   8df05:	4a 8b 54 c7 08       	mov    0x8(%rdi,%r8,8),%rdx
   8df0a:	f2 42 0f 10 44 c7 10 	movsd  0x10(%rdi,%r8,8),%xmm0
   8df11:	48 89 b4 24 b0 02 00 	mov    %rsi,0x2b0(%rsp)
   8df18:	00 
   8df19:	48 89 94 24 b8 02 00 	mov    %rdx,0x2b8(%rsp)
   8df20:	00 
   8df21:	f2 0f 11 84 24 c0 02 	movsd  %xmm0,0x2c0(%rsp)
   8df28:	00 00 
   8df2a:	48 8d 7e f4          	lea    -0xc(%rsi),%rdi
   8df2e:	48 83 ff f8          	cmp    $0xfffffffffffffff8,%rdi
   8df32:	0f 82 9d 45 00 00    	jb     924d5 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x7085>
   8df38:	48 85 d2             	test   %rdx,%rdx
   8df3b:	0f 88 94 45 00 00    	js     924d5 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x7085>
   8df41:	89 d7                	mov    %edx,%edi
   8df43:	49 3b 7a 08          	cmp    0x8(%r10),%rdi
   8df47:	0f 8d 88 45 00 00    	jge    924d5 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x7085>
   8df4d:	4d 8b 0a             	mov    (%r10),%r9
   8df50:	4c 69 c7 60 01 00 00 	imul   $0x160,%rdi,%r8
   8df57:	4b 39 34 01          	cmp    %rsi,(%r9,%r8,1)
   8df5b:	0f 85 74 45 00 00    	jne    924d5 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x7085>
   8df61:	48 c1 ea 20          	shr    $0x20,%rdx
   8df65:	4b 39 54 01 19       	cmp    %rdx,0x19(%r9,%r8,1)
   8df6a:	0f 85 65 45 00 00    	jne    924d5 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x7085>
   8df70:	48 39 c7             	cmp    %rax,%rdi
   8df73:	0f 83 4c 43 00 00    	jae    922c5 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6e75>
   8df79:	4e 8b a4 01 89 00 00 	mov    0x89(%rcx,%r8,1),%r12
   8df80:	00 
   8df81:	4c 89 f7             	mov    %r14,%rdi
   8df84:	48 8d b4 24 b0 02 00 	lea    0x2b0(%rsp),%rsi
   8df8b:	00 
   8df8c:	48 8d 94 24 98 02 00 	lea    0x298(%rsp),%rdx
   8df93:	00 
   8df94:	e8 17 e2 06 00       	call   fc1b0 <_Wfl.fl_closure_new$04ec049e49b3b2ad>
   8df99:	48 8b b4 24 98 02 00 	mov    0x298(%rsp),%rsi
   8dfa0:	00 
   8dfa1:	48 8d 46 fc          	lea    -0x4(%rsi),%rax
   8dfa5:	48 83 f8 07          	cmp    $0x7,%rax
   8dfa9:	0f 87 8c 44 00 00    	ja     9243b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6feb>
   8dfaf:	48 8b bc 24 a0 02 00 	mov    0x2a0(%rsp),%rdi
   8dfb6:	00 
   8dfb7:	48 85 ff             	test   %rdi,%rdi
   8dfba:	0f 88 7b 44 00 00    	js     9243b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6feb>
   8dfc0:	49 8b 06             	mov    (%r14),%rax
   8dfc3:	89 fd                	mov    %edi,%ebp
   8dfc5:	48 8b 48 08          	mov    0x8(%rax),%rcx
   8dfc9:	48 39 cd             	cmp    %rcx,%rbp
   8dfcc:	0f 8d 69 44 00 00    	jge    9243b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6feb>
   8dfd2:	48 8b 00             	mov    (%rax),%rax
   8dfd5:	48 69 dd 60 01 00 00 	imul   $0x160,%rbp,%rbx
   8dfdc:	48 39 34 18          	cmp    %rsi,(%rax,%rbx,1)
   8dfe0:	0f 85 55 44 00 00    	jne    9243b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6feb>
   8dfe6:	48 89 fa             	mov    %rdi,%rdx
   8dfe9:	48 c1 ea 20          	shr    $0x20,%rdx
   8dfed:	48 39 54 18 19       	cmp    %rdx,0x19(%rax,%rbx,1)
   8dff2:	0f 85 43 44 00 00    	jne    9243b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6feb>
   8dff8:	48 89 7c 24 20       	mov    %rdi,0x20(%rsp)
   8dffd:	48 89 74 24 28       	mov    %rsi,0x28(%rsp)
   8e002:	48 8b 54 24 78       	mov    0x78(%rsp),%rdx
   8e007:	48 39 ca             	cmp    %rcx,%rdx
   8e00a:	0f 83 5f 45 00 00    	jae    9256f <_Wfl.fl_vm_exec$a5081fe92876eb37+0x711f>
   8e010:	49 8b 4e 40          	mov    0x40(%r14),%rcx
   8e014:	48 69 d2 60 01 00 00 	imul   $0x160,%rdx,%rdx
   8e01b:	f2 0f 10 84 10 f1 00 	movsd  0xf1(%rax,%rdx,1),%xmm0
   8e022:	00 00 
   8e024:	48 89 54 24 50       	mov    %rdx,0x50(%rsp)
   8e029:	0f 10 8c 10 e1 00 00 	movups 0xe1(%rax,%rdx,1),%xmm1
   8e030:	00 
   8e031:	0f 11 8c 18 e1 00 00 	movups %xmm1,0xe1(%rax,%rbx,1)
   8e038:	00 
   8e039:	f2 0f 11 84 18 f1 00 	movsd  %xmm0,0xf1(%rax,%rbx,1)
   8e040:	00 00 
   8e042:	4c 89 a4 18 89 00 00 	mov    %r12,0x89(%rax,%rbx,1)
   8e049:	00 
   8e04a:	48 b8 00 00 00 00 00 	movabs $0xf000000000000000,%rax
   8e051:	00 00 f0 
   8e054:	4c 01 e0             	add    %r12,%rax
   8e057:	48 c1 e8 3d          	shr    $0x3d,%rax
   8e05b:	83 f8 07             	cmp    $0x7,%eax
   8e05e:	0f 82 1b 3c 00 00    	jb     91c7f <_Wfl.fl_vm_exec$a5081fe92876eb37+0x682f>
   8e064:	4a 8d 04 e5 00 00 00 	lea    0x0(,%r12,8),%rax
   8e06b:	00 
   8e06c:	48 01 c1             	add    %rax,%rcx
   8e06f:	0f 80 0a 3c 00 00    	jo     91c7f <_Wfl.fl_vm_exec$a5081fe92876eb37+0x682f>
   8e075:	49 89 4e 40          	mov    %rcx,0x40(%r14)
   8e079:	4d 85 e4             	test   %r12,%r12
   8e07c:	4c 8b 5c 24 18       	mov    0x18(%rsp),%r11
   8e081:	4c 8b 7c 24 10       	mov    0x10(%rsp),%r15
   8e086:	4c 8b ac 24 80 00 00 	mov    0x80(%rsp),%r13
   8e08d:	00 
   8e08e:	7f 7b                	jg     8e10b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x2cbb>
   8e090:	49 8b 86 ac 00 00 00 	mov    0xac(%r14),%rax
   8e097:	48 3d 00 40 00 00    	cmp    $0x4000,%rax
   8e09d:	0f 8d ac 04 00 00    	jge    8e54f <_Wfl.fl_vm_exec$a5081fe92876eb37+0x30ff>
   8e0a3:	49 8b 8e a4 00 00 00 	mov    0xa4(%r14),%rcx
   8e0aa:	48 3b 41 08          	cmp    0x8(%rcx),%rax
   8e0ae:	0f 83 e6 47 00 00    	jae    9289a <_Wfl.fl_vm_exec$a5081fe92876eb37+0x744a>
   8e0b4:	48 8b 09             	mov    (%rcx),%rcx
   8e0b7:	48 8d 14 40          	lea    (%rax,%rax,2),%rdx
   8e0bb:	48 8b 74 24 28       	mov    0x28(%rsp),%rsi
   8e0c0:	48 89 34 d1          	mov    %rsi,(%rcx,%rdx,8)
   8e0c4:	48 8b 74 24 20       	mov    0x20(%rsp),%rsi
   8e0c9:	48 89 74 d1 08       	mov    %rsi,0x8(%rcx,%rdx,8)
   8e0ce:	f2 0f 10 84 24 a8 02 	movsd  0x2a8(%rsp),%xmm0
   8e0d5:	00 00 
   8e0d7:	e9 2b 0e 00 00       	jmp    8ef07 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x3ab7>
   8e0dc:	48 89 8c 24 30 02 00 	mov    %rcx,0x230(%rsp)
   8e0e3:	00 
   8e0e4:	48 89 94 24 38 02 00 	mov    %rdx,0x238(%rsp)
   8e0eb:	00 
   8e0ec:	48 89 b4 24 40 02 00 	mov    %rsi,0x240(%rsp)
   8e0f3:	00 
   8e0f4:	48 8d b4 24 30 02 00 	lea    0x230(%rsp),%rsi
   8e0fb:	00 
   8e0fc:	e8 4f 09 0c 00       	call   14ea50 <__wolf_rt_list_push>
   8e101:	4c 8b 5c 24 18       	mov    0x18(%rsp),%r11
   8e106:	49 ff cc             	dec    %r12
   8e109:	74 85                	je     8e090 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x2c40>
   8e10b:	49 8b 86 b4 00 00 00 	mov    0xb4(%r14),%rax
   8e112:	4c 3b 68 08          	cmp    0x8(%rax),%r13
   8e116:	0f 83 b2 3e 00 00    	jae    91fce <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6b7e>
   8e11c:	48 8b 30             	mov    (%rax),%rsi
   8e11f:	4e 8b 44 1e 18       	mov    0x18(%rsi,%r11,1),%r8
   8e124:	4c 89 c2             	mov    %r8,%rdx
   8e127:	48 ff c2             	inc    %rdx
   8e12a:	0f 80 27 37 00 00    	jo     91857 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6407>
   8e130:	49 8b 06             	mov    (%r14),%rax
   8e133:	4a 89 54 1e 18       	mov    %rdx,0x18(%rsi,%r11,1)
   8e138:	48 8b 48 08          	mov    0x8(%rax),%rcx
   8e13c:	48 39 0c 24          	cmp    %rcx,(%rsp)
   8e140:	0f 83 25 3e 00 00    	jae    91f6b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6b1b>
   8e146:	48 8b 38             	mov    (%rax),%rdi
   8e149:	4e 8b 4c 3f 61       	mov    0x61(%rdi,%r15,1),%r9
   8e14e:	4d 3b 41 08          	cmp    0x8(%r9),%r8
   8e152:	0f 83 13 3e 00 00    	jae    91f6b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6b1b>
   8e158:	49 89 d2             	mov    %rdx,%r10
   8e15b:	49 ff c2             	inc    %r10
   8e15e:	0f 80 14 37 00 00    	jo     91878 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6428>
   8e164:	4d 8b 09             	mov    (%r9),%r9
   8e167:	47 0f b6 04 01       	movzbl (%r9,%r8,1),%r8d
   8e16c:	4e 89 54 1e 18       	mov    %r10,0x18(%rsi,%r11,1)
   8e171:	4a 8b 74 3f 61       	mov    0x61(%rdi,%r15,1),%rsi
   8e176:	48 3b 56 08          	cmp    0x8(%rsi),%rdx
   8e17a:	0f 83 a6 3e 00 00    	jae    92026 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6bd6>
   8e180:	48 8b 36             	mov    (%rsi),%rsi
   8e183:	0f b6 34 16          	movzbl (%rsi,%rdx,1),%esi
   8e187:	45 84 c0             	test   %r8b,%r8b
   8e18a:	74 37                	je     8e1c3 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x2d73>
   8e18c:	48 03 74 24 68       	add    0x68(%rsp),%rsi
   8e191:	0f 80 44 37 00 00    	jo     918db <_Wfl.fl_vm_exec$a5081fe92876eb37+0x648b>
   8e197:	4c 89 f7             	mov    %r14,%rdi
   8e19a:	4c 8d bc 24 a8 04 00 	lea    0x4a8(%rsp),%r15
   8e1a1:	00 
   8e1a2:	4c 89 fa             	mov    %r15,%rdx
   8e1a5:	e8 46 a0 01 00       	call   a81f0 <_Wfl.fl_capture_upval$a2bcd0b46b228105>
   8e1aa:	48 8b 94 24 b0 04 00 	mov    0x4b0(%rsp),%rdx
   8e1b1:	00 
   8e1b2:	48 8b b4 24 b8 04 00 	mov    0x4b8(%rsp),%rsi
   8e1b9:	00 
   8e1ba:	49 8b 06             	mov    (%r14),%rax
   8e1bd:	48 8b 48 08          	mov    0x8(%rax),%rcx
   8e1c1:	eb 34                	jmp    8e1f7 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x2da7>
   8e1c3:	48 39 4c 24 78       	cmp    %rcx,0x78(%rsp)
   8e1c8:	0f 83 e7 3e 00 00    	jae    920b5 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6c65>
   8e1ce:	48 8b 54 24 50       	mov    0x50(%rsp),%rdx
   8e1d3:	48 8b 54 17 29       	mov    0x29(%rdi,%rdx,1),%rdx
   8e1d8:	48 39 72 08          	cmp    %rsi,0x8(%rdx)
   8e1dc:	0f 86 d3 3e 00 00    	jbe    920b5 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6c65>
   8e1e2:	4c 8b 02             	mov    (%rdx),%r8
   8e1e5:	48 8d 34 76          	lea    (%rsi,%rsi,2),%rsi
   8e1e9:	4d 8d 3c f0          	lea    (%r8,%rsi,8),%r15
   8e1ed:	49 8b 54 f0 08       	mov    0x8(%r8,%rsi,8),%rdx
   8e1f2:	49 8b 74 f0 10       	mov    0x10(%r8,%rsi,8),%rsi
   8e1f7:	48 39 cd             	cmp    %rcx,%rbp
   8e1fa:	0f 83 5d 3e 00 00    	jae    9205d <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6c0d>
   8e200:	49 8b 0f             	mov    (%r15),%rcx
   8e203:	48 8b 00             	mov    (%rax),%rax
   8e206:	48 8b 7c 18 29       	mov    0x29(%rax,%rbx,1),%rdi
   8e20b:	48 8b 47 08          	mov    0x8(%rdi),%rax
   8e20f:	48 3b 47 10          	cmp    0x10(%rdi),%rax
   8e213:	4c 8b 7c 24 10       	mov    0x10(%rsp),%r15
   8e218:	0f 83 be fe ff ff    	jae    8e0dc <_Wfl.fl_vm_exec$a5081fe92876eb37+0x2c8c>
   8e21e:	4c 8b 07             	mov    (%rdi),%r8
   8e221:	4c 8d 0c 40          	lea    (%rax,%rax,2),%r9
   8e225:	4b 89 0c c8          	mov    %rcx,(%r8,%r9,8)
   8e229:	4b 89 54 c8 08       	mov    %rdx,0x8(%r8,%r9,8)
   8e22e:	4b 89 74 c8 10       	mov    %rsi,0x10(%r8,%r9,8)
   8e233:	48 ff c0             	inc    %rax
   8e236:	48 89 47 08          	mov    %rax,0x8(%rdi)
   8e23a:	e9 c2 fe ff ff       	jmp    8e101 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x2cb1>
   8e23f:	4d 3b 6f 08          	cmp    0x8(%r15),%r13
   8e243:	0f 83 5a 44 00 00    	jae    926a3 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x7253>
   8e249:	49 8b 0f             	mov    (%r15),%rcx
   8e24c:	48 8b 74 24 18       	mov    0x18(%rsp),%rsi
   8e251:	48 8b 44 31 18       	mov    0x18(%rcx,%rsi,1),%rax
   8e256:	48 89 c2             	mov    %rax,%rdx
   8e259:	48 83 c2 02          	add    $0x2,%rdx
   8e25d:	0f 80 c1 3a 00 00    	jo     91d24 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x68d4>
   8e263:	48 89 54 31 18       	mov    %rdx,0x18(%rcx,%rsi,1)
   8e268:	48 8b 0c 24          	mov    (%rsp),%rcx
   8e26c:	49 3b 4a 08          	cmp    0x8(%r10),%rcx
   8e270:	0f 83 cf 44 00 00    	jae    92745 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x72f5>
   8e276:	49 8b 0a             	mov    (%r10),%rcx
   8e279:	48 8b 54 24 10       	mov    0x10(%rsp),%rdx
   8e27e:	48 8b 4c 11 61       	mov    0x61(%rcx,%rdx,1),%rcx
   8e283:	48 8b 51 08          	mov    0x8(%rcx),%rdx
   8e287:	48 39 d0             	cmp    %rdx,%rax
   8e28a:	0f 83 b5 44 00 00    	jae    92745 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x72f5>
   8e290:	48 8d 70 01          	lea    0x1(%rax),%rsi
   8e294:	48 39 d6             	cmp    %rdx,%rsi
   8e297:	0f 83 a8 44 00 00    	jae    92745 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x72f5>
   8e29d:	48 8b 09             	mov    (%rcx),%rcx
   8e2a0:	44 0f b6 24 01       	movzbl (%rcx,%rax,1),%r12d
   8e2a5:	0f b6 5c 01 01       	movzbl 0x1(%rcx,%rax,1),%ebx
   8e2aa:	c1 e3 08             	shl    $0x8,%ebx
   8e2ad:	be 05 00 00 00       	mov    $0x5,%esi
   8e2b2:	ba 28 00 00 00       	mov    $0x28,%edx
   8e2b7:	4c 89 f7             	mov    %r14,%rdi
   8e2ba:	4c 8d ac 24 80 02 00 	lea    0x280(%rsp),%r13
   8e2c1:	00 
   8e2c2:	4c 89 e9             	mov    %r13,%rcx
   8e2c5:	e8 e6 67 ff ff       	call   84ab0 <_Wfl.fl_gc_alloc$34adf57915ed2b38>
   8e2ca:	49 89 df             	mov    %rbx,%r15
   8e2cd:	4d 09 e7             	or     %r12,%r15
   8e2d0:	48 8d ac 24 f0 03 00 	lea    0x3f0(%rsp),%rbp
   8e2d7:	00 
   8e2d8:	74 68                	je     8e342 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x2ef2>
   8e2da:	4c 01 e3             	add    %r12,%rbx
   8e2dd:	45 31 e4             	xor    %r12d,%r12d
   8e2e0:	49 8b 86 ac 00 00 00 	mov    0xac(%r14),%rax
   8e2e7:	4c 29 f8             	sub    %r15,%rax
   8e2ea:	0f 80 8b 34 00 00    	jo     9177b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x632b>
   8e2f0:	4c 01 e0             	add    %r12,%rax
   8e2f3:	0f 80 82 34 00 00    	jo     9177b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x632b>
   8e2f9:	49 8b 8e a4 00 00 00 	mov    0xa4(%r14),%rcx
   8e300:	48 3b 41 08          	cmp    0x8(%rcx),%rax
   8e304:	0f 83 69 3d 00 00    	jae    92073 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6c23>
   8e30a:	48 8b 09             	mov    (%rcx),%rcx
   8e30d:	48 8d 04 40          	lea    (%rax,%rax,2),%rax
   8e311:	f2 0f 10 44 c1 10    	movsd  0x10(%rcx,%rax,8),%xmm0
   8e317:	0f 10 0c c1          	movups (%rcx,%rax,8),%xmm1
   8e31b:	0f 29 8c 24 f0 03 00 	movaps %xmm1,0x3f0(%rsp)
   8e322:	00 
   8e323:	f2 0f 11 84 24 00 04 	movsd  %xmm0,0x400(%rsp)
   8e32a:	00 00 
   8e32c:	4c 89 f7             	mov    %r14,%rdi
   8e32f:	4c 89 ee             	mov    %r13,%rsi
   8e332:	48 89 ea             	mov    %rbp,%rdx
   8e335:	e8 46 10 03 00       	call   bf380 <_Wfl.fl_list_push$255051f5e64d80eb>
   8e33a:	49 ff c4             	inc    %r12
   8e33d:	4c 39 e3             	cmp    %r12,%rbx
   8e340:	75 9e                	jne    8e2e0 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x2e90>
   8e342:	49 8b 86 ac 00 00 00 	mov    0xac(%r14),%rax
   8e349:	4c 29 f8             	sub    %r15,%rax
   8e34c:	0f 80 fe 39 00 00    	jo     91d50 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6900>
   8e352:	49 89 86 ac 00 00 00 	mov    %rax,0xac(%r14)
   8e359:	48 3d 00 40 00 00    	cmp    $0x4000,%rax
   8e35f:	0f 8d ea 01 00 00    	jge    8e54f <_Wfl.fl_vm_exec$a5081fe92876eb37+0x30ff>
   8e365:	49 8b 8e a4 00 00 00 	mov    0xa4(%r14),%rcx
   8e36c:	48 3b 41 08          	cmp    0x8(%rcx),%rax
   8e370:	0f 83 45 45 00 00    	jae    928bb <_Wfl.fl_vm_exec$a5081fe92876eb37+0x746b>
   8e376:	48 8b 09             	mov    (%rcx),%rcx
   8e379:	48 8d 14 40          	lea    (%rax,%rax,2),%rdx
   8e37d:	48 8b b4 24 80 02 00 	mov    0x280(%rsp),%rsi
   8e384:	00 
   8e385:	48 89 34 d1          	mov    %rsi,(%rcx,%rdx,8)
   8e389:	48 8b b4 24 88 02 00 	mov    0x288(%rsp),%rsi
   8e390:	00 
   8e391:	48 89 74 d1 08       	mov    %rsi,0x8(%rcx,%rdx,8)
   8e396:	f2 0f 10 84 24 90 02 	movsd  0x290(%rsp),%xmm0
   8e39d:	00 00 
   8e39f:	e9 63 0b 00 00       	jmp    8ef07 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x3ab7>
   8e3a4:	3c 2c                	cmp    $0x2c,%al
   8e3a6:	0f 85 83 09 00 00    	jne    8ed2f <_Wfl.fl_vm_exec$a5081fe92876eb37+0x38df>
   8e3ac:	4d 3b 6f 08          	cmp    0x8(%r15),%r13
   8e3b0:	0f 83 9b 40 00 00    	jae    92451 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x7001>
   8e3b6:	49 8b 0f             	mov    (%r15),%rcx
   8e3b9:	48 8b 44 24 18       	mov    0x18(%rsp),%rax
   8e3be:	48 8b 44 01 18       	mov    0x18(%rcx,%rax,1),%rax
   8e3c3:	48 89 c2             	mov    %rax,%rdx
   8e3c6:	48 83 c2 02          	add    $0x2,%rdx
   8e3ca:	0f 80 01 3b 00 00    	jo     91ed1 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6a81>
   8e3d0:	48 8b 74 24 18       	mov    0x18(%rsp),%rsi
   8e3d5:	48 89 54 31 18       	mov    %rdx,0x18(%rcx,%rsi,1)
   8e3da:	48 8b 0c 24          	mov    (%rsp),%rcx
   8e3de:	49 3b 4a 08          	cmp    0x8(%r10),%rcx
   8e3e2:	0f 83 e5 3f 00 00    	jae    923cd <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6f7d>
   8e3e8:	49 8b 12             	mov    (%r10),%rdx
   8e3eb:	48 8b 4c 24 10       	mov    0x10(%rsp),%rcx
   8e3f0:	48 8b 4c 0a 61       	mov    0x61(%rdx,%rcx,1),%rcx
   8e3f5:	48 8b 71 08          	mov    0x8(%rcx),%rsi
   8e3f9:	48 39 f0             	cmp    %rsi,%rax
   8e3fc:	0f 83 cb 3f 00 00    	jae    923cd <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6f7d>
   8e402:	48 8d 78 01          	lea    0x1(%rax),%rdi
   8e406:	48 39 f7             	cmp    %rsi,%rdi
   8e409:	0f 83 be 3f 00 00    	jae    923cd <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6f7d>
   8e40f:	48 8b 74 24 10       	mov    0x10(%rsp),%rsi
   8e414:	48 8b 54 32 29       	mov    0x29(%rdx,%rsi,1),%rdx
   8e419:	48 8b 09             	mov    (%rcx),%rcx
   8e41c:	0f b7 04 01          	movzwl (%rcx,%rax,1),%eax
   8e420:	48 3b 42 08          	cmp    0x8(%rdx),%rax
   8e424:	0f 83 77 3f 00 00    	jae    923a1 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6f51>
   8e42a:	48 8b 0a             	mov    (%rdx),%rcx
   8e42d:	48 8d 04 40          	lea    (%rax,%rax,2),%rax
   8e431:	4c 8b 7c c1 08       	mov    0x8(%rcx,%rax,8),%r15
   8e436:	80 7c 24 28 2d       	cmpb   $0x2d,0x28(%rsp)
   8e43b:	0f 85 e4 0a 00 00    	jne    8ef25 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x3ad5>
   8e441:	48 83 7c 24 20 00    	cmpq   $0x0,0x20(%rsp)
   8e447:	7f 14                	jg     8e45d <_Wfl.fl_vm_exec$a5081fe92876eb37+0x300d>
   8e449:	ba 16 00 00 00       	mov    $0x16,%edx
   8e44e:	4c 89 f7             	mov    %r14,%rdi
   8e451:	48 8d 35 8b 64 f8 ff 	lea    -0x79b75(%rip),%rsi        # 148e3 <__abi_tag+0x145e7>
   8e458:	e8 a3 af 02 00       	call   b9400 <_Wfl.fl_current_bug$22c9f442c8960c68>
   8e45d:	49 8b 86 ac 00 00 00 	mov    0xac(%r14),%rax
   8e464:	48 ff c8             	dec    %rax
   8e467:	0f 80 7a 3a 00 00    	jo     91ee7 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6a97>
   8e46d:	49 89 86 ac 00 00 00 	mov    %rax,0xac(%r14)
   8e474:	49 8b 8e a4 00 00 00 	mov    0xa4(%r14),%rcx
   8e47b:	48 3b 41 08          	cmp    0x8(%rcx),%rax
   8e47f:	0f 83 56 3e 00 00    	jae    922db <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6e8b>
   8e485:	48 8b 09             	mov    (%rcx),%rcx
   8e488:	48 8d 14 40          	lea    (%rax,%rax,2),%rdx
   8e48c:	48 8b 74 d1 10       	mov    0x10(%rcx,%rdx,8),%rsi
   8e491:	0f 10 04 d1          	movups (%rcx,%rdx,8),%xmm0
   8e495:	0f 29 84 24 50 01 00 	movaps %xmm0,0x150(%rsp)
   8e49c:	00 
   8e49d:	48 89 b4 24 60 01 00 	mov    %rsi,0x160(%rsp)
   8e4a4:	00 
   8e4a5:	e9 99 0a 00 00       	jmp    8ef43 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x3af3>
   8e4aa:	e8 c1 aa 0c 00       	call   158f70 <__wolf_rt_strbuf_new>
   8e4af:	49 89 c7             	mov    %rax,%r15
   8e4b2:	48 89 df             	mov    %rbx,%rdi
   8e4b5:	e8 e6 d5 06 00       	call   fbaa0 <_Wfl.fl_type_name$425de879c2cc2a41>
   8e4ba:	4c 89 ff             	mov    %r15,%rdi
   8e4bd:	48 89 c6             	mov    %rax,%rsi
   8e4c0:	31 c9                	xor    %ecx,%ecx
   8e4c2:	e8 59 ab 0c 00       	call   159020 <__wolf_rt_strbuf_str>
   8e4c7:	ba 17 00 00 00       	mov    $0x17,%edx
   8e4cc:	4c 89 ff             	mov    %r15,%rdi
   8e4cf:	48 8d 35 9d 64 f8 ff 	lea    -0x79b63(%rip),%rsi        # 14973 <__abi_tag+0x14677>
   8e4d6:	31 c9                	xor    %ecx,%ecx
   8e4d8:	e8 43 ab 0c 00       	call   159020 <__wolf_rt_strbuf_str>
   8e4dd:	4c 89 ff             	mov    %r15,%rdi
   8e4e0:	48 8d b4 24 f0 01 00 	lea    0x1f0(%rsp),%rsi
   8e4e7:	00 
   8e4e8:	e8 33 a7 0c 00       	call   158c20 <__wolf_rt_strbuf_finish>
   8e4ed:	48 8b 8c 24 f0 01 00 	mov    0x1f0(%rsp),%rcx
   8e4f4:	00 
   8e4f5:	4c 8b 84 24 f8 01 00 	mov    0x1f8(%rsp),%r8
   8e4fc:	00 
   8e4fd:	ba 03 00 00 00       	mov    $0x3,%edx
   8e502:	4c 89 f7             	mov    %r14,%rdi
   8e505:	48 8d 35 93 62 f8 ff 	lea    -0x79d6d(%rip),%rsi        # 1479f <__abi_tag+0x144a3>
   8e50c:	4c 8d 8c 24 90 1f 00 	lea    0x1f90(%rsp),%r9
   8e513:	00 
   8e514:	e8 b7 45 08 00       	call   112ad0 <_Wfl.fl_raise_bytes.bytesview.4$e8b5fa06fc8adc2d>
   8e519:	49 8d 86 5c 01 00 00 	lea    0x15c(%r14),%rax
   8e520:	80 38 00             	cmpb   $0x0,(%rax)
   8e523:	0f 85 ee 09 00 00    	jne    8ef17 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x3ac7>
   8e529:	49 8b 86 ac 00 00 00 	mov    0xac(%r14),%rax
   8e530:	4d 01 e4             	add    %r12,%r12
   8e533:	4c 29 e0             	sub    %r12,%rax
   8e536:	0f 80 11 39 00 00    	jo     91e4d <_Wfl.fl_vm_exec$a5081fe92876eb37+0x69fd>
   8e53c:	49 89 86 ac 00 00 00 	mov    %rax,0xac(%r14)
   8e543:	48 3d 00 40 00 00    	cmp    $0x4000,%rax
   8e549:	0f 8c 7e 09 00 00    	jl     8eecd <_Wfl.fl_vm_exec$a5081fe92876eb37+0x3a7d>
   8e54f:	4c 89 f7             	mov    %r14,%rdi
   8e552:	e8 09 90 02 00       	call   b7560 <_Wfl.fl_push_overflow$ec8cb5daf6580c79>
   8e557:	e9 bb 09 00 00       	jmp    8ef17 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x3ac7>
   8e55c:	0f 57 c0             	xorps  %xmm0,%xmm0
   8e55f:	0f 29 84 24 e0 00 00 	movaps %xmm0,0xe0(%rsp)
   8e566:	00 
   8e567:	48 c7 84 24 f0 00 00 	movq   $0x0,0xf0(%rsp)
   8e56e:	00 00 00 00 00 
   8e573:	49 8b 86 ac 00 00 00 	mov    0xac(%r14),%rax
   8e57a:	48 85 c0             	test   %rax,%rax
   8e57d:	7f 1b                	jg     8e59a <_Wfl.fl_vm_exec$a5081fe92876eb37+0x314a>
   8e57f:	ba 16 00 00 00       	mov    $0x16,%edx
   8e584:	4c 89 f7             	mov    %r14,%rdi
   8e587:	48 8d 35 55 63 f8 ff 	lea    -0x79cab(%rip),%rsi        # 148e3 <__abi_tag+0x145e7>
   8e58e:	e8 6d ae 02 00       	call   b9400 <_Wfl.fl_current_bug$22c9f442c8960c68>
   8e593:	49 8b 86 ac 00 00 00 	mov    0xac(%r14),%rax
   8e59a:	48 ff c8             	dec    %rax
   8e59d:	0f 80 73 38 00 00    	jo     91e16 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x69c6>
   8e5a3:	49 89 86 ac 00 00 00 	mov    %rax,0xac(%r14)
   8e5aa:	49 8b 8e a4 00 00 00 	mov    0xa4(%r14),%rcx
   8e5b1:	48 3b 41 08          	cmp    0x8(%rcx),%rax
   8e5b5:	0f 83 a0 41 00 00    	jae    9275b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x730b>
   8e5bb:	48 8b 09             	mov    (%rcx),%rcx
   8e5be:	48 8d 14 40          	lea    (%rax,%rax,2),%rdx
   8e5c2:	4c 8b 3c d1          	mov    (%rcx,%rdx,8),%r15
   8e5c6:	48 8b 74 d1 08       	mov    0x8(%rcx,%rdx,8),%rsi
   8e5cb:	f2 0f 10 44 d1 10    	movsd  0x10(%rcx,%rdx,8),%xmm0
   8e5d1:	4c 89 bc 24 38 01 00 	mov    %r15,0x138(%rsp)
   8e5d8:	00 
   8e5d9:	48 89 f5             	mov    %rsi,%rbp
   8e5dc:	48 89 b4 24 40 01 00 	mov    %rsi,0x140(%rsp)
   8e5e3:	00 
   8e5e4:	f2 0f 11 84 24 48 01 	movsd  %xmm0,0x148(%rsp)
   8e5eb:	00 00 
   8e5ed:	48 85 c0             	test   %rax,%rax
   8e5f0:	7f 1b                	jg     8e60d <_Wfl.fl_vm_exec$a5081fe92876eb37+0x31bd>
   8e5f2:	ba 16 00 00 00       	mov    $0x16,%edx
   8e5f7:	4c 89 f7             	mov    %r14,%rdi
   8e5fa:	48 8d 35 e2 62 f8 ff 	lea    -0x79d1e(%rip),%rsi        # 148e3 <__abi_tag+0x145e7>
   8e601:	e8 fa ad 02 00       	call   b9400 <_Wfl.fl_current_bug$22c9f442c8960c68>
   8e606:	49 8b 86 ac 00 00 00 	mov    0xac(%r14),%rax
   8e60d:	48 ff c8             	dec    %rax
   8e610:	0f 80 ea 37 00 00    	jo     91e00 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x69b0>
   8e616:	49 89 86 ac 00 00 00 	mov    %rax,0xac(%r14)
   8e61d:	49 8b 76 20          	mov    0x20(%r14),%rsi
   8e621:	49 8b 56 48          	mov    0x48(%r14),%rdx
   8e625:	41 0f b7 4e 50       	movzwl 0x50(%r14),%ecx
   8e62a:	41 0f 10 06          	movups (%r14),%xmm0
   8e62e:	0f 29 84 24 f0 09 00 	movaps %xmm0,0x9f0(%rsp)
   8e635:	00 
   8e636:	41 0f 10 46 10       	movups 0x10(%r14),%xmm0
   8e63b:	0f 29 84 24 00 0a 00 	movaps %xmm0,0xa00(%rsp)
   8e642:	00 
   8e643:	48 89 b4 24 10 0a 00 	mov    %rsi,0xa10(%rsp)
   8e64a:	00 
   8e64b:	49 8b 76 62          	mov    0x62(%r14),%rsi
   8e64f:	41 0f 10 46 28       	movups 0x28(%r14),%xmm0
   8e654:	0f 11 84 24 18 0a 00 	movups %xmm0,0xa18(%rsp)
   8e65b:	00 
   8e65c:	41 0f 10 46 38       	movups 0x38(%r14),%xmm0
   8e661:	0f 11 84 24 28 0a 00 	movups %xmm0,0xa28(%rsp)
   8e668:	00 
   8e669:	48 89 94 24 38 0a 00 	mov    %rdx,0xa38(%rsp)
   8e670:	00 
   8e671:	49 8b 96 8a 00 00 00 	mov    0x8a(%r14),%rdx
   8e678:	48 89 8c 24 40 0a 00 	mov    %rcx,0xa40(%rsp)
   8e67f:	00 
   8e680:	41 0f b7 be a2 00 00 	movzwl 0xa2(%r14),%edi
   8e687:	00 
   8e688:	41 0f 10 46 52       	movups 0x52(%r14),%xmm0
   8e68d:	0f 11 84 24 48 0a 00 	movups %xmm0,0xa48(%rsp)
   8e694:	00 
   8e695:	48 89 b4 24 58 0a 00 	mov    %rsi,0xa58(%rsp)
   8e69c:	00 
   8e69d:	49 8b 8e a4 00 00 00 	mov    0xa4(%r14),%rcx
   8e6a4:	41 0f 10 46 6a       	movups 0x6a(%r14),%xmm0
   8e6a9:	0f 29 84 24 60 0a 00 	movaps %xmm0,0xa60(%rsp)
   8e6b0:	00 
   8e6b1:	41 0f 10 46 7a       	movups 0x7a(%r14),%xmm0
   8e6b6:	0f 29 84 24 70 0a 00 	movaps %xmm0,0xa70(%rsp)
   8e6bd:	00 
   8e6be:	48 89 94 24 80 0a 00 	mov    %rdx,0xa80(%rsp)
   8e6c5:	00 
   8e6c6:	49 8b b6 c4 00 00 00 	mov    0xc4(%r14),%rsi
   8e6cd:	41 0f 10 86 92 00 00 	movups 0x92(%r14),%xmm0
   8e6d4:	00 
   8e6d5:	0f 11 84 24 88 0a 00 	movups %xmm0,0xa88(%rsp)
   8e6dc:	00 
   8e6dd:	48 89 bc 24 98 0a 00 	mov    %rdi,0xa98(%rsp)
   8e6e4:	00 
   8e6e5:	41 0f b6 96 5c 01 00 	movzbl 0x15c(%r14),%edx
   8e6ec:	00 
   8e6ed:	48 89 8c 24 a0 0a 00 	mov    %rcx,0xaa0(%rsp)
   8e6f4:	00 
   8e6f5:	48 89 84 24 a8 0a 00 	mov    %rax,0xaa8(%rsp)
   8e6fc:	00 
   8e6fd:	41 0f 10 86 b4 00 00 	movups 0xb4(%r14),%xmm0
   8e704:	00 
   8e705:	0f 29 84 24 b0 0a 00 	movaps %xmm0,0xab0(%rsp)
   8e70c:	00 
   8e70d:	41 0f 10 86 cc 00 00 	movups 0xcc(%r14),%xmm0
   8e714:	00 
   8e715:	48 89 b4 24 c0 0a 00 	mov    %rsi,0xac0(%rsp)
   8e71c:	00 
   8e71d:	0f 11 84 24 c8 0a 00 	movups %xmm0,0xac8(%rsp)
   8e724:	00 
   8e725:	41 0f 10 86 dc 00 00 	movups 0xdc(%r14),%xmm0
   8e72c:	00 
   8e72d:	0f 11 84 24 d8 0a 00 	movups %xmm0,0xad8(%rsp)
   8e734:	00 
   8e735:	41 0f 10 86 ec 00 00 	movups 0xec(%r14),%xmm0
   8e73c:	00 
   8e73d:	0f 11 84 24 e8 0a 00 	movups %xmm0,0xae8(%rsp)
   8e744:	00 
   8e745:	41 0f 10 86 fc 00 00 	movups 0xfc(%r14),%xmm0
   8e74c:	00 
   8e74d:	0f 11 84 24 f8 0a 00 	movups %xmm0,0xaf8(%rsp)
   8e754:	00 
   8e755:	41 0f 10 86 0c 01 00 	movups 0x10c(%r14),%xmm0
   8e75c:	00 
   8e75d:	0f 11 84 24 08 0b 00 	movups %xmm0,0xb08(%rsp)
   8e764:	00 
   8e765:	41 0f 10 86 1c 01 00 	movups 0x11c(%r14),%xmm0
   8e76c:	00 
   8e76d:	0f 11 84 24 18 0b 00 	movups %xmm0,0xb18(%rsp)
   8e774:	00 
   8e775:	41 0f 10 86 2c 01 00 	movups 0x12c(%r14),%xmm0
   8e77c:	00 
   8e77d:	0f 11 84 24 28 0b 00 	movups %xmm0,0xb28(%rsp)
   8e784:	00 
   8e785:	41 0f 10 86 3c 01 00 	movups 0x13c(%r14),%xmm0
   8e78c:	00 
   8e78d:	0f 11 84 24 38 0b 00 	movups %xmm0,0xb38(%rsp)
   8e794:	00 
   8e795:	41 0f 10 86 4c 01 00 	movups 0x14c(%r14),%xmm0
   8e79c:	00 
   8e79d:	0f 11 84 24 48 0b 00 	movups %xmm0,0xb48(%rsp)
   8e7a4:	00 
   8e7a5:	41 0f 10 86 5d 01 00 	movups 0x15d(%r14),%xmm0
   8e7ac:	00 
   8e7ad:	88 94 24 58 0b 00 00 	mov    %dl,0xb58(%rsp)
   8e7b4:	0f 29 84 24 60 0b 00 	movaps %xmm0,0xb60(%rsp)
   8e7bb:	00 
   8e7bc:	41 0f 10 86 6d 01 00 	movups 0x16d(%r14),%xmm0
   8e7c3:	00 
   8e7c4:	41 0f b6 96 dd 01 00 	movzbl 0x1dd(%r14),%edx
   8e7cb:	00 
   8e7cc:	49 8b b6 ee 01 00 00 	mov    0x1ee(%r14),%rsi
   8e7d3:	0f 29 84 24 70 0b 00 	movaps %xmm0,0xb70(%rsp)
   8e7da:	00 
   8e7db:	41 0f 10 86 7d 01 00 	movups 0x17d(%r14),%xmm0
   8e7e2:	00 
   8e7e3:	0f 29 84 24 80 0b 00 	movaps %xmm0,0xb80(%rsp)
   8e7ea:	00 
   8e7eb:	41 0f 10 86 8d 01 00 	movups 0x18d(%r14),%xmm0
   8e7f2:	00 
   8e7f3:	0f 29 84 24 90 0b 00 	movaps %xmm0,0xb90(%rsp)
   8e7fa:	00 
   8e7fb:	41 0f 10 86 9d 01 00 	movups 0x19d(%r14),%xmm0
   8e802:	00 
   8e803:	0f 29 84 24 a0 0b 00 	movaps %xmm0,0xba0(%rsp)
   8e80a:	00 
   8e80b:	41 0f 10 86 ad 01 00 	movups 0x1ad(%r14),%xmm0
   8e812:	00 
   8e813:	0f 29 84 24 b0 0b 00 	movaps %xmm0,0xbb0(%rsp)
   8e81a:	00 
   8e81b:	41 0f 10 86 bd 01 00 	movups 0x1bd(%r14),%xmm0
   8e822:	00 
   8e823:	0f 29 84 24 c0 0b 00 	movaps %xmm0,0xbc0(%rsp)
   8e82a:	00 
   8e82b:	41 0f 10 86 cd 01 00 	movups 0x1cd(%r14),%xmm0
   8e832:	00 
   8e833:	0f 29 84 24 d0 0b 00 	movaps %xmm0,0xbd0(%rsp)
   8e83a:	00 
   8e83b:	88 94 24 e0 0b 00 00 	mov    %dl,0xbe0(%rsp)
   8e842:	49 8b 96 16 02 00 00 	mov    0x216(%r14),%rdx
   8e849:	41 0f 10 86 de 01 00 	movups 0x1de(%r14),%xmm0
   8e850:	00 
   8e851:	0f 11 84 24 e8 0b 00 	movups %xmm0,0xbe8(%rsp)
   8e858:	00 
   8e859:	48 89 b4 24 f8 0b 00 	mov    %rsi,0xbf8(%rsp)
   8e860:	00 
   8e861:	41 0f b6 b6 1e 02 00 	movzbl 0x21e(%r14),%esi
   8e868:	00 
   8e869:	41 0f 10 86 f6 01 00 	movups 0x1f6(%r14),%xmm0
   8e870:	00 
   8e871:	0f 29 84 24 00 0c 00 	movaps %xmm0,0xc00(%rsp)
   8e878:	00 
   8e879:	41 0f 10 86 06 02 00 	movups 0x206(%r14),%xmm0
   8e880:	00 
   8e881:	0f 29 84 24 10 0c 00 	movaps %xmm0,0xc10(%rsp)
   8e888:	00 
   8e889:	48 89 94 24 20 0c 00 	mov    %rdx,0xc20(%rsp)
   8e890:	00 
   8e891:	49 8b 96 2f 02 00 00 	mov    0x22f(%r14),%rdx
   8e898:	48 89 b4 24 28 0c 00 	mov    %rsi,0xc28(%rsp)
   8e89f:	00 
   8e8a0:	49 8b b6 47 02 00 00 	mov    0x247(%r14),%rsi
   8e8a7:	41 0f 10 86 1f 02 00 	movups 0x21f(%r14),%xmm0
   8e8ae:	00 
   8e8af:	0f 29 84 24 30 0c 00 	movaps %xmm0,0xc30(%rsp)
   8e8b6:	00 
   8e8b7:	48 89 94 24 40 0c 00 	mov    %rdx,0xc40(%rsp)
   8e8be:	00 
   8e8bf:	49 8b 96 7f 02 00 00 	mov    0x27f(%r14),%rdx
   8e8c6:	41 0f 10 86 37 02 00 	movups 0x237(%r14),%xmm0
   8e8cd:	00 
   8e8ce:	0f 11 84 24 48 0c 00 	movups %xmm0,0xc48(%rsp)
   8e8d5:	00 
   8e8d6:	41 0f 10 86 4f 02 00 	movups 0x24f(%r14),%xmm0
   8e8dd:	00 
   8e8de:	48 89 b4 24 58 0c 00 	mov    %rsi,0xc58(%rsp)
   8e8e5:	00 
   8e8e6:	0f 29 84 24 60 0c 00 	movaps %xmm0,0xc60(%rsp)
   8e8ed:	00 
   8e8ee:	41 0f 10 86 5f 02 00 	movups 0x25f(%r14),%xmm0
   8e8f5:	00 
   8e8f6:	0f 29 84 24 70 0c 00 	movaps %xmm0,0xc70(%rsp)
   8e8fd:	00 
   8e8fe:	41 0f 10 86 6f 02 00 	movups 0x26f(%r14),%xmm0
   8e905:	00 
   8e906:	0f 29 84 24 80 0c 00 	movaps %xmm0,0xc80(%rsp)
   8e90d:	00 
   8e90e:	41 0f 10 86 87 02 00 	movups 0x287(%r14),%xmm0
   8e915:	00 
   8e916:	48 89 94 24 90 0c 00 	mov    %rdx,0xc90(%rsp)
   8e91d:	00 
   8e91e:	0f 11 84 24 98 0c 00 	movups %xmm0,0xc98(%rsp)
   8e925:	00 
   8e926:	41 0f 10 86 97 02 00 	movups 0x297(%r14),%xmm0
   8e92d:	00 
   8e92e:	0f 11 84 24 a8 0c 00 	movups %xmm0,0xca8(%rsp)
   8e935:	00 
   8e936:	41 0f 10 86 a7 02 00 	movups 0x2a7(%r14),%xmm0
   8e93d:	00 
   8e93e:	0f 11 84 24 b8 0c 00 	movups %xmm0,0xcb8(%rsp)
   8e945:	00 
   8e946:	41 0f 10 86 b7 02 00 	movups 0x2b7(%r14),%xmm0
   8e94d:	00 
   8e94e:	0f 11 84 24 c8 0c 00 	movups %xmm0,0xcc8(%rsp)
   8e955:	00 
   8e956:	41 0f 10 86 c7 02 00 	movups 0x2c7(%r14),%xmm0
   8e95d:	00 
   8e95e:	0f 11 84 24 d8 0c 00 	movups %xmm0,0xcd8(%rsp)
   8e965:	00 
   8e966:	41 0f 10 86 d7 02 00 	movups 0x2d7(%r14),%xmm0
   8e96d:	00 
   8e96e:	0f 11 84 24 e8 0c 00 	movups %xmm0,0xce8(%rsp)
   8e975:	00 
   8e976:	41 0f 10 86 e7 02 00 	movups 0x2e7(%r14),%xmm0
   8e97d:	00 
   8e97e:	0f 11 84 24 f8 0c 00 	movups %xmm0,0xcf8(%rsp)
   8e985:	00 
   8e986:	41 0f 10 86 f7 02 00 	movups 0x2f7(%r14),%xmm0
   8e98d:	00 
   8e98e:	0f 11 84 24 08 0d 00 	movups %xmm0,0xd08(%rsp)
   8e995:	00 
   8e996:	41 0f 10 86 07 03 00 	movups 0x307(%r14),%xmm0
   8e99d:	00 
   8e99e:	49 8b 96 17 03 00 00 	mov    0x317(%r14),%rdx
   8e9a5:	49 8b b6 3f 03 00 00 	mov    0x33f(%r14),%rsi
   8e9ac:	0f 11 84 24 18 0d 00 	movups %xmm0,0xd18(%rsp)
   8e9b3:	00 
   8e9b4:	48 89 94 24 28 0d 00 	mov    %rdx,0xd28(%rsp)
   8e9bb:	00 
   8e9bc:	49 8b 96 57 03 00 00 	mov    0x357(%r14),%rdx
   8e9c3:	41 0f 10 86 1f 03 00 	movups 0x31f(%r14),%xmm0
   8e9ca:	00 
   8e9cb:	0f 29 84 24 30 0d 00 	movaps %xmm0,0xd30(%rsp)
   8e9d2:	00 
   8e9d3:	41 0f 10 86 2f 03 00 	movups 0x32f(%r14),%xmm0
   8e9da:	00 
   8e9db:	0f 29 84 24 40 0d 00 	movaps %xmm0,0xd40(%rsp)
   8e9e2:	00 
   8e9e3:	48 89 b4 24 50 0d 00 	mov    %rsi,0xd50(%rsp)
   8e9ea:	00 
   8e9eb:	41 0f b6 b6 5f 03 00 	movzbl 0x35f(%r14),%esi
   8e9f2:	00 
   8e9f3:	41 0f 10 86 47 03 00 	movups 0x347(%r14),%xmm0
   8e9fa:	00 
   8e9fb:	0f 11 84 24 58 0d 00 	movups %xmm0,0xd58(%rsp)
   8ea02:	00 
   8ea03:	48 89 94 24 68 0d 00 	mov    %rdx,0xd68(%rsp)
   8ea0a:	00 
   8ea0b:	49 8b 96 90 03 00 00 	mov    0x390(%r14),%rdx
   8ea12:	48 89 b4 24 70 0d 00 	mov    %rsi,0xd70(%rsp)
   8ea19:	00 
   8ea1a:	41 0f b6 b6 a8 03 00 	movzbl 0x3a8(%r14),%esi
   8ea21:	00 
   8ea22:	41 0f 10 86 60 03 00 	movups 0x360(%r14),%xmm0
   8ea29:	00 
   8ea2a:	0f 11 84 24 78 0d 00 	movups %xmm0,0xd78(%rsp)
   8ea31:	00 
   8ea32:	41 0f 10 86 70 03 00 	movups 0x370(%r14),%xmm0
   8ea39:	00 
   8ea3a:	0f 11 84 24 88 0d 00 	movups %xmm0,0xd88(%rsp)
   8ea41:	00 
   8ea42:	41 0f 10 86 80 03 00 	movups 0x380(%r14),%xmm0
   8ea49:	00 
   8ea4a:	0f 11 84 24 98 0d 00 	movups %xmm0,0xd98(%rsp)
   8ea51:	00 
   8ea52:	48 89 94 24 a8 0d 00 	mov    %rdx,0xda8(%rsp)
   8ea59:	00 
   8ea5a:	49 8b 96 b9 03 00 00 	mov    0x3b9(%r14),%rdx
   8ea61:	41 0f 10 86 98 03 00 	movups 0x398(%r14),%xmm0
   8ea68:	00 
   8ea69:	0f 29 84 24 b0 0d 00 	movaps %xmm0,0xdb0(%rsp)
   8ea70:	00 
   8ea71:	48 89 b4 24 c0 0d 00 	mov    %rsi,0xdc0(%rsp)
   8ea78:	00 
   8ea79:	49 8b b6 01 04 00 00 	mov    0x401(%r14),%rsi
   8ea80:	41 0f 10 86 a9 03 00 	movups 0x3a9(%r14),%xmm0
   8ea87:	00 
   8ea88:	0f 11 84 24 c8 0d 00 	movups %xmm0,0xdc8(%rsp)
   8ea8f:	00 
   8ea90:	41 0f 10 86 c1 03 00 	movups 0x3c1(%r14),%xmm0
   8ea97:	00 
   8ea98:	48 89 94 24 d8 0d 00 	mov    %rdx,0xdd8(%rsp)
   8ea9f:	00 
   8eaa0:	0f 29 84 24 e0 0d 00 	movaps %xmm0,0xde0(%rsp)
   8eaa7:	00 
   8eaa8:	41 0f 10 86 d1 03 00 	movups 0x3d1(%r14),%xmm0
   8eaaf:	00 
   8eab0:	0f 29 84 24 f0 0d 00 	movaps %xmm0,0xdf0(%rsp)
   8eab7:	00 
   8eab8:	41 0f 10 86 e1 03 00 	movups 0x3e1(%r14),%xmm0
   8eabf:	00 
   8eac0:	0f 29 84 24 00 0e 00 	movaps %xmm0,0xe00(%rsp)
   8eac7:	00 
   8eac8:	41 0f 10 86 f1 03 00 	movups 0x3f1(%r14),%xmm0
   8eacf:	00 
   8ead0:	0f 29 84 24 10 0e 00 	movaps %xmm0,0xe10(%rsp)
   8ead7:	00 
   8ead8:	41 0f 10 86 09 04 00 	movups 0x409(%r14),%xmm0
   8eadf:	00 
   8eae0:	48 89 b4 24 20 0e 00 	mov    %rsi,0xe20(%rsp)
   8eae7:	00 
   8eae8:	0f 11 84 24 28 0e 00 	movups %xmm0,0xe28(%rsp)
   8eaef:	00 
   8eaf0:	41 0f 10 86 19 04 00 	movups 0x419(%r14),%xmm0
   8eaf7:	00 
   8eaf8:	41 0f b6 96 69 04 00 	movzbl 0x469(%r14),%edx
   8eaff:	00 
   8eb00:	41 0f b7 b6 6a 04 00 	movzwl 0x46a(%r14),%esi
   8eb07:	00 
   8eb08:	c1 e6 08             	shl    $0x8,%esi
   8eb0b:	48 09 d6             	or     %rdx,%rsi
   8eb0e:	49 8b 96 49 04 00 00 	mov    0x449(%r14),%rdx
   8eb15:	0f 11 84 24 38 0e 00 	movups %xmm0,0xe38(%rsp)
   8eb1c:	00 
   8eb1d:	41 0f 10 86 29 04 00 	movups 0x429(%r14),%xmm0
   8eb24:	00 
   8eb25:	0f 11 84 24 48 0e 00 	movups %xmm0,0xe48(%rsp)
   8eb2c:	00 
   8eb2d:	41 0f 10 86 39 04 00 	movups 0x439(%r14),%xmm0
   8eb34:	00 
   8eb35:	0f 11 84 24 58 0e 00 	movups %xmm0,0xe58(%rsp)
   8eb3c:	00 
   8eb3d:	48 89 94 24 68 0e 00 	mov    %rdx,0xe68(%rsp)
   8eb44:	00 
   8eb45:	49 8b 96 61 04 00 00 	mov    0x461(%r14),%rdx
   8eb4c:	41 0f 10 86 51 04 00 	movups 0x451(%r14),%xmm0
   8eb53:	00 
   8eb54:	0f 29 84 24 70 0e 00 	movaps %xmm0,0xe70(%rsp)
   8eb5b:	00 
   8eb5c:	48 89 94 24 80 0e 00 	mov    %rdx,0xe80(%rsp)
   8eb63:	00 
   8eb64:	41 0f 10 86 6c 04 00 	movups 0x46c(%r14),%xmm0
   8eb6b:	00 
   8eb6c:	48 89 b4 24 88 0e 00 	mov    %rsi,0xe88(%rsp)
   8eb73:	00 
   8eb74:	0f 29 84 24 90 0e 00 	movaps %xmm0,0xe90(%rsp)
   8eb7b:	00 
   8eb7c:	41 0f 10 86 7c 04 00 	movups 0x47c(%r14),%xmm0
   8eb83:	00 
   8eb84:	41 0f b7 96 8c 04 00 	movzwl 0x48c(%r14),%edx
   8eb8b:	00 
   8eb8c:	0f 29 84 24 a0 0e 00 	movaps %xmm0,0xea0(%rsp)
   8eb93:	00 
   8eb94:	41 0f 10 86 8e 04 00 	movups 0x48e(%r14),%xmm0
   8eb9b:	00 
   8eb9c:	48 89 94 24 b0 0e 00 	mov    %rdx,0xeb0(%rsp)
   8eba3:	00 
   8eba4:	0f 11 84 24 b8 0e 00 	movups %xmm0,0xeb8(%rsp)
   8ebab:	00 
   8ebac:	41 0f 10 86 9e 04 00 	movups 0x49e(%r14),%xmm0
   8ebb3:	00 
   8ebb4:	41 0f b6 96 ae 04 00 	movzbl 0x4ae(%r14),%edx
   8ebbb:	00 
   8ebbc:	0f 11 84 24 c8 0e 00 	movups %xmm0,0xec8(%rsp)
   8ebc3:	00 
   8ebc4:	41 0f 10 86 af 04 00 	movups 0x4af(%r14),%xmm0
   8ebcb:	00 
   8ebcc:	88 94 24 d8 0e 00 00 	mov    %dl,0xed8(%rsp)
   8ebd3:	0f 29 84 24 e0 0e 00 	movaps %xmm0,0xee0(%rsp)
   8ebda:	00 
   8ebdb:	41 0f 10 86 bf 04 00 	movups 0x4bf(%r14),%xmm0
   8ebe2:	00 
   8ebe3:	0f 29 84 24 f0 0e 00 	movaps %xmm0,0xef0(%rsp)
   8ebea:	00 
   8ebeb:	41 0f 10 86 cf 04 00 	movups 0x4cf(%r14),%xmm0
   8ebf2:	00 
   8ebf3:	0f 29 84 24 00 0f 00 	movaps %xmm0,0xf00(%rsp)
   8ebfa:	00 
   8ebfb:	49 8b 96 df 04 00 00 	mov    0x4df(%r14),%rdx
   8ec02:	48 89 94 24 10 0f 00 	mov    %rdx,0xf10(%rsp)
   8ec09:	00 
   8ec0a:	4d 8b 2e             	mov    (%r14),%r13
   8ec0d:	48 3b 41 08          	cmp    0x8(%rcx),%rax
   8ec11:	0f 83 bd 3b 00 00    	jae    927d4 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x7384>
   8ec17:	80 7c 24 28 2a       	cmpb   $0x2a,0x28(%rsp)
   8ec1c:	40 0f 95 c7          	setne  %dil
   8ec20:	4c 8b 01             	mov    (%rcx),%r8
   8ec23:	48 8d 14 c5 00 00 00 	lea    0x0(,%rax,8),%rdx
   8ec2a:	00 
   8ec2b:	48 8d 14 52          	lea    (%rdx,%rdx,2),%rdx
   8ec2f:	4d 8b 24 10          	mov    (%r8,%rdx,1),%r12
   8ec33:	49 8b 74 10 08       	mov    0x8(%r8,%rdx,1),%rsi
   8ec38:	f2 41 0f 10 44 10 10 	movsd  0x10(%r8,%rdx,1),%xmm0
   8ec3f:	4c 89 a4 24 68 01 00 	mov    %r12,0x168(%rsp)
   8ec46:	00 
   8ec47:	48 89 b4 24 70 01 00 	mov    %rsi,0x170(%rsp)
   8ec4e:	00 
   8ec4f:	f2 0f 11 84 24 78 01 	movsd  %xmm0,0x178(%rsp)
   8ec56:	00 00 
   8ec58:	49 83 ff 02          	cmp    $0x2,%r15
   8ec5c:	41 0f 95 c0          	setne  %r8b
   8ec60:	49 83 fc 05          	cmp    $0x5,%r12
   8ec64:	0f 85 b5 01 00 00    	jne    8ee1f <_Wfl.fl_vm_exec$a5081fe92876eb37+0x39cf>
   8ec6a:	44 20 c7             	and    %r8b,%dil
   8ec6d:	0f 85 ac 01 00 00    	jne    8ee1f <_Wfl.fl_vm_exec$a5081fe92876eb37+0x39cf>
   8ec73:	48 85 f6             	test   %rsi,%rsi
   8ec76:	0f 88 a9 37 00 00    	js     92425 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6fd5>
   8ec7c:	41 89 f0             	mov    %esi,%r8d
   8ec7f:	4d 3b 45 08          	cmp    0x8(%r13),%r8
   8ec83:	0f 8d 9c 37 00 00    	jge    92425 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6fd5>
   8ec89:	49 8b 7d 00          	mov    0x0(%r13),%rdi
   8ec8d:	4d 69 c0 60 01 00 00 	imul   $0x160,%r8,%r8
   8ec94:	4a 83 3c 07 05       	cmpq   $0x5,(%rdi,%r8,1)
   8ec99:	0f 85 86 37 00 00    	jne    92425 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6fd5>
   8ec9f:	48 c1 ee 20          	shr    $0x20,%rsi
   8eca3:	4a 39 74 07 19       	cmp    %rsi,0x19(%rdi,%r8,1)
   8eca8:	0f 85 77 37 00 00    	jne    92425 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6fd5>
   8ecae:	49 83 ff 02          	cmp    $0x2,%r15
   8ecb2:	0f 85 fe 03 00 00    	jne    8f0b6 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x3c66>
   8ecb8:	4a 8b 74 07 29       	mov    0x29(%rdi,%r8,1),%rsi
   8ecbd:	4c 8b 7e 08          	mov    0x8(%rsi),%r15
   8ecc1:	48 85 ed             	test   %rbp,%rbp
   8ecc4:	0f 88 84 04 00 00    	js     8f14e <_Wfl.fl_vm_exec$a5081fe92876eb37+0x3cfe>
   8ecca:	4c 39 fd             	cmp    %r15,%rbp
   8eccd:	0f 8d 7b 04 00 00    	jge    8f14e <_Wfl.fl_vm_exec$a5081fe92876eb37+0x3cfe>
   8ecd3:	80 7c 24 28 2a       	cmpb   $0x2a,0x28(%rsp)
   8ecd8:	0f 84 d0 05 00 00    	je     8f2ae <_Wfl.fl_vm_exec$a5081fe92876eb37+0x3e5e>
   8ecde:	4c 89 f7             	mov    %r14,%rdi
   8ece1:	48 8d b4 24 68 01 00 	lea    0x168(%rsp),%rsi
   8ece8:	00 
   8ece9:	48 89 ea             	mov    %rbp,%rdx
   8ecec:	48 8d 8c 24 e0 00 00 	lea    0xe0(%rsp),%rcx
   8ecf3:	00 
   8ecf4:	e8 a7 ce 06 00       	call   fbba0 <_Wfl.fl_list_set$6ba3fca6afac6ce9>
   8ecf9:	84 c0                	test   %al,%al
   8ecfb:	0f 85 16 02 00 00    	jne    8ef17 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x3ac7>
   8ed01:	ba 0c 00 00 00       	mov    $0xc,%edx
   8ed06:	4c 89 f7             	mov    %r14,%rdi
   8ed09:	48 8d 35 30 5b f8 ff 	lea    -0x7a4d0(%rip),%rsi        # 14840 <__abi_tag+0x14544>
   8ed10:	4c 8b 24 24          	mov    (%rsp),%r12
   8ed14:	4c 89 e1             	mov    %r12,%rcx
   8ed17:	4c 8b 44 24 70       	mov    0x70(%rsp),%r8
   8ed1c:	4c 8b 4c 24 50       	mov    0x50(%rsp),%r9
   8ed21:	6a 24                	push   $0x24
   8ed23:	48 8d 05 92 5c f8 ff 	lea    -0x7a36e(%rip),%rax        # 149bc <__abi_tag+0x146c0>
   8ed2a:	e9 89 12 00 00       	jmp    8ffb8 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4b68>
   8ed2f:	48 8b 44 24 50       	mov    0x50(%rsp),%rax
   8ed34:	83 c0 d2             	add    $0xffffffd2,%eax
   8ed37:	83 f8 0d             	cmp    $0xd,%eax
   8ed3a:	0f 87 e9 07 00 00    	ja     8f529 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x40d9>
   8ed40:	48 89 f5             	mov    %rsi,%rbp
   8ed43:	48 8d 0d 02 54 f8 ff 	lea    -0x7abfe(%rip),%rcx        # 1414c <__abi_tag+0x13e50>
   8ed4a:	48 63 04 81          	movslq (%rcx,%rax,4),%rax
   8ed4e:	48 01 c8             	add    %rcx,%rax
   8ed51:	ff e0                	jmp    *%rax
   8ed53:	4d 3b 6f 08          	cmp    0x8(%r15),%r13
   8ed57:	0f 83 9f 35 00 00    	jae    922fc <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6eac>
   8ed5d:	49 8b 0f             	mov    (%r15),%rcx
   8ed60:	48 8b 44 24 18       	mov    0x18(%rsp),%rax
   8ed65:	48 8b 54 01 18       	mov    0x18(%rcx,%rax,1),%rdx
   8ed6a:	48 89 d0             	mov    %rdx,%rax
   8ed6d:	48 ff c0             	inc    %rax
   8ed70:	0f 80 9a 32 00 00    	jo     92010 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6bc0>
   8ed76:	48 8b 74 24 18       	mov    0x18(%rsp),%rsi
   8ed7b:	48 89 44 31 18       	mov    %rax,0x18(%rcx,%rsi,1)
   8ed80:	48 8b 34 24          	mov    (%rsp),%rsi
   8ed84:	49 3b 72 08          	cmp    0x8(%r10),%rsi
   8ed88:	0f 83 cb 37 00 00    	jae    92559 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x7109>
   8ed8e:	49 8b 32             	mov    (%r10),%rsi
   8ed91:	48 8b 7c 24 10       	mov    0x10(%rsp),%rdi
   8ed96:	48 8b 7c 3e 61       	mov    0x61(%rsi,%rdi,1),%rdi
   8ed9b:	48 3b 57 08          	cmp    0x8(%rdi),%rdx
   8ed9f:	0f 83 b4 37 00 00    	jae    92559 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x7109>
   8eda5:	49 89 c0             	mov    %rax,%r8
   8eda8:	49 83 c0 02          	add    $0x2,%r8
   8edac:	0f 80 32 32 00 00    	jo     91fe4 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6b94>
   8edb2:	48 8b 3f             	mov    (%rdi),%rdi
   8edb5:	0f b6 14 17          	movzbl (%rdi,%rdx,1),%edx
   8edb9:	48 8b 7c 24 18       	mov    0x18(%rsp),%rdi
   8edbe:	4c 89 44 39 18       	mov    %r8,0x18(%rcx,%rdi,1)
   8edc3:	48 8b 4c 24 10       	mov    0x10(%rsp),%rcx
   8edc8:	48 8b 4c 0e 61       	mov    0x61(%rsi,%rcx,1),%rcx
   8edcd:	48 8b 79 08          	mov    0x8(%rcx),%rdi
   8edd1:	48 39 f8             	cmp    %rdi,%rax
   8edd4:	0f 83 ed 37 00 00    	jae    925c7 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x7177>
   8edda:	48 8d 70 01          	lea    0x1(%rax),%rsi
   8edde:	48 39 fe             	cmp    %rdi,%rsi
   8ede1:	0f 83 e0 37 00 00    	jae    925c7 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x7177>
   8ede7:	48 03 54 24 68       	add    0x68(%rsp),%rdx
   8edec:	0f 80 08 32 00 00    	jo     91ffa <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6baa>
   8edf2:	48 8b 09             	mov    (%rcx),%rcx
   8edf5:	0f b6 04 01          	movzbl (%rcx,%rax,1),%eax
   8edf9:	0f b6 0c 31          	movzbl (%rcx,%rsi,1),%ecx
   8edfd:	c1 e1 08             	shl    $0x8,%ecx
   8ee00:	48 09 c1             	or     %rax,%rcx
   8ee03:	45 31 c0             	xor    %r8d,%r8d
   8ee06:	80 7c 24 28 30       	cmpb   $0x30,0x28(%rsp)
   8ee0b:	41 0f 94 c0          	sete   %r8b
   8ee0f:	4c 89 f7             	mov    %r14,%rdi
   8ee12:	4c 89 ee             	mov    %r13,%rsi
   8ee15:	e8 16 b9 ff ff       	call   8a730 <_Wfl.fl_iter_next$312c49f98a86acc1>
   8ee1a:	e9 f8 00 00 00       	jmp    8ef17 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x3ac7>
   8ee1f:	49 83 fc 06          	cmp    $0x6,%r12
   8ee23:	0f 85 36 02 00 00    	jne    8f05f <_Wfl.fl_vm_exec$a5081fe92876eb37+0x3c0f>
   8ee29:	80 7c 24 28 2a       	cmpb   $0x2a,0x28(%rsp)
   8ee2e:	0f 84 0b 18 00 00    	je     9063f <_Wfl.fl_vm_exec$a5081fe92876eb37+0x51ef>
   8ee34:	4c 89 f7             	mov    %r14,%rdi
   8ee37:	48 8d b4 24 68 01 00 	lea    0x168(%rsp),%rsi
   8ee3e:	00 
   8ee3f:	48 8d 94 24 38 01 00 	lea    0x138(%rsp),%rdx
   8ee46:	00 
   8ee47:	48 8d 8c 24 e0 00 00 	lea    0xe0(%rsp),%rcx
   8ee4e:	00 
   8ee4f:	e8 5c 07 03 00       	call   bf5b0 <_Wfl.fl_map_set$540fcc92758450a0>
   8ee54:	84 c0                	test   %al,%al
   8ee56:	0f 85 bb 00 00 00    	jne    8ef17 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x3ac7>
   8ee5c:	e8 0f a1 0c 00       	call   158f70 <__wolf_rt_strbuf_new>
   8ee61:	48 89 c3             	mov    %rax,%rbx
   8ee64:	4c 89 ff             	mov    %r15,%rdi
   8ee67:	e8 34 cc 06 00       	call   fbaa0 <_Wfl.fl_type_name$425de879c2cc2a41>
   8ee6c:	48 89 df             	mov    %rbx,%rdi
   8ee6f:	48 89 c6             	mov    %rax,%rsi
   8ee72:	31 c9                	xor    %ecx,%ecx
   8ee74:	e8 a7 a1 0c 00       	call   159020 <__wolf_rt_strbuf_str>
   8ee79:	ba 17 00 00 00       	mov    $0x17,%edx
   8ee7e:	48 89 df             	mov    %rbx,%rdi
   8ee81:	48 8d 35 eb 5a f8 ff 	lea    -0x7a515(%rip),%rsi        # 14973 <__abi_tag+0x14677>
   8ee88:	31 c9                	xor    %ecx,%ecx
   8ee8a:	e8 91 a1 0c 00       	call   159020 <__wolf_rt_strbuf_str>
   8ee8f:	48 89 df             	mov    %rbx,%rdi
   8ee92:	48 8d b4 24 d0 01 00 	lea    0x1d0(%rsp),%rsi
   8ee99:	00 
   8ee9a:	e8 81 9d 0c 00       	call   158c20 <__wolf_rt_strbuf_finish>
   8ee9f:	48 8b 8c 24 d0 01 00 	mov    0x1d0(%rsp),%rcx
   8eea6:	00 
   8eea7:	4c 8b 84 24 d8 01 00 	mov    0x1d8(%rsp),%r8
   8eeae:	00 
   8eeaf:	ba 03 00 00 00       	mov    $0x3,%edx
   8eeb4:	4c 89 f7             	mov    %r14,%rdi
   8eeb7:	48 8d 35 e1 58 f8 ff 	lea    -0x7a71f(%rip),%rsi        # 1479f <__abi_tag+0x144a3>
   8eebe:	4c 8d 8c 24 30 1f 00 	lea    0x1f30(%rsp),%r9
   8eec5:	00 
   8eec6:	e8 05 3c 08 00       	call   112ad0 <_Wfl.fl_raise_bytes.bytesview.4$e8b5fa06fc8adc2d>
   8eecb:	eb 4a                	jmp    8ef17 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x3ac7>
   8eecd:	49 8b 8e a4 00 00 00 	mov    0xa4(%r14),%rcx
   8eed4:	48 3b 41 08          	cmp    0x8(%rcx),%rax
   8eed8:	0f 83 15 37 00 00    	jae    925f3 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x71a3>
   8eede:	48 8b 09             	mov    (%rcx),%rcx
   8eee1:	48 8d 14 40          	lea    (%rax,%rax,2),%rdx
   8eee5:	48 8b b4 24 80 01 00 	mov    0x180(%rsp),%rsi
   8eeec:	00 
   8eeed:	48 89 34 d1          	mov    %rsi,(%rcx,%rdx,8)
   8eef1:	48 8b b4 24 88 01 00 	mov    0x188(%rsp),%rsi
   8eef8:	00 
   8eef9:	48 89 74 d1 08       	mov    %rsi,0x8(%rcx,%rdx,8)
   8eefe:	f2 0f 10 84 24 90 01 	movsd  0x190(%rsp),%xmm0
   8ef05:	00 00 
   8ef07:	f2 0f 11 44 d1 10    	movsd  %xmm0,0x10(%rcx,%rdx,8)
   8ef0d:	48 ff c0             	inc    %rax
   8ef10:	49 89 86 ac 00 00 00 	mov    %rax,0xac(%r14)
   8ef17:	48 8b 6c 24 08       	mov    0x8(%rsp),%rbp
   8ef1c:	4c 8b 24 24          	mov    (%rsp),%r12
   8ef20:	e9 36 c6 ff ff       	jmp    8b55b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x10b>
   8ef25:	0f 57 c0             	xorps  %xmm0,%xmm0
   8ef28:	0f 29 84 24 50 01 00 	movaps %xmm0,0x150(%rsp)
   8ef2f:	00 
   8ef30:	48 c7 84 24 60 01 00 	movq   $0x0,0x160(%rsp)
   8ef37:	00 00 00 00 00 
   8ef3c:	49 8b 86 ac 00 00 00 	mov    0xac(%r14),%rax
   8ef43:	48 85 c0             	test   %rax,%rax
   8ef46:	7f 1b                	jg     8ef63 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x3b13>
   8ef48:	ba 16 00 00 00       	mov    $0x16,%edx
   8ef4d:	4c 89 f7             	mov    %r14,%rdi
   8ef50:	48 8d 35 8c 59 f8 ff 	lea    -0x7a674(%rip),%rsi        # 148e3 <__abi_tag+0x145e7>
   8ef57:	e8 a4 a4 02 00       	call   b9400 <_Wfl.fl_current_bug$22c9f442c8960c68>
   8ef5c:	49 8b 86 ac 00 00 00 	mov    0xac(%r14),%rax
   8ef63:	48 ff c8             	dec    %rax
   8ef66:	0f 80 4f 2f 00 00    	jo     91ebb <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6a6b>
   8ef6c:	49 89 86 ac 00 00 00 	mov    %rax,0xac(%r14)
   8ef73:	49 8b 96 a4 00 00 00 	mov    0xa4(%r14),%rdx
   8ef7a:	48 3b 42 08          	cmp    0x8(%rdx),%rax
   8ef7e:	0f 83 08 3a 00 00    	jae    9298c <_Wfl.fl_vm_exec$a5081fe92876eb37+0x753c>
   8ef84:	49 8b 4e 6a          	mov    0x6a(%r14),%rcx
   8ef88:	48 8b 12             	mov    (%rdx),%rdx
   8ef8b:	48 8d 04 40          	lea    (%rax,%rax,2),%rax
   8ef8f:	48 8b 1c c2          	mov    (%rdx,%rax,8),%rbx
   8ef93:	4c 8b 64 c2 08       	mov    0x8(%rdx,%rax,8),%r12
   8ef98:	f2 0f 10 44 c2 10    	movsd  0x10(%rdx,%rax,8),%xmm0
   8ef9e:	48 89 9c 24 20 01 00 	mov    %rbx,0x120(%rsp)
   8efa5:	00 
   8efa6:	4c 89 a4 24 28 01 00 	mov    %r12,0x128(%rsp)
   8efad:	00 
   8efae:	f2 0f 11 84 24 30 01 	movsd  %xmm0,0x130(%rsp)
   8efb5:	00 00 
   8efb7:	4c 3b 79 08          	cmp    0x8(%rcx),%r15
   8efbb:	0f 83 da 35 00 00    	jae    9259b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x714b>
   8efc1:	48 8b 01             	mov    (%rcx),%rax
   8efc4:	4a 8b 3c f8          	mov    (%rax,%r15,8),%rdi
   8efc8:	48 8d b4 24 c0 01 00 	lea    0x1c0(%rsp),%rsi
   8efcf:	00 
   8efd0:	e8 db 97 0c 00       	call   1587b0 <__wolf_rt_str_from_utf8>
   8efd5:	48 85 c0             	test   %rax,%rax
   8efd8:	0f 85 7f 0e 00 00    	jne    8fe5d <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4a0d>
   8efde:	4c 8b ac 24 c0 01 00 	mov    0x1c0(%rsp),%r13
   8efe5:	00 
   8efe6:	4c 8b bc 24 c8 01 00 	mov    0x1c8(%rsp),%r15
   8efed:	00 
   8efee:	80 7c 24 28 2c       	cmpb   $0x2c,0x28(%rsp)
   8eff3:	0f 85 7c 0e 00 00    	jne    8fe75 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4a25>
   8eff9:	48 83 fb 06          	cmp    $0x6,%rbx
   8effd:	0f 84 72 0e 00 00    	je     8fe75 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4a25>
   8f003:	4c 89 f7             	mov    %r14,%rdi
   8f006:	48 8d b4 24 20 01 00 	lea    0x120(%rsp),%rsi
   8f00d:	00 
   8f00e:	4c 89 ea             	mov    %r13,%rdx
   8f011:	4c 89 f9             	mov    %r15,%rcx
   8f014:	4c 8d 84 24 30 03 00 	lea    0x330(%rsp),%r8
   8f01b:	00 
   8f01c:	e8 cf 2a 06 00       	call   f1af0 <_Wfl.fl_api_bind_receiver.bytesview.4$fea5acaf256156a6>
   8f021:	48 83 bc 24 38 03 00 	cmpq   $0x0,0x338(%rsp)
   8f028:	00 00 
   8f02a:	0f 94 c0             	sete   %al
   8f02d:	48 83 bc 24 30 03 00 	cmpq   $0x0,0x330(%rsp)
   8f034:	00 00 
   8f036:	0f 85 9e 16 00 00    	jne    906da <_Wfl.fl_vm_exec$a5081fe92876eb37+0x528a>
   8f03c:	84 c0                	test   %al,%al
   8f03e:	0f 84 a0 16 00 00    	je     906e4 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5294>
   8f044:	4c 89 f7             	mov    %r14,%rdi
   8f047:	4c 89 ee             	mov    %r13,%rsi
   8f04a:	4c 89 fa             	mov    %r15,%rdx
   8f04d:	48 8d 8c 24 b8 00 00 	lea    0xb8(%rsp),%rcx
   8f054:	00 
   8f055:	e8 86 79 05 00       	call   e69e0 <_Wfl.fl_str_text$a33697b96957ba85>
   8f05a:	e9 8d 0f 00 00       	jmp    8ffec <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4b9c>
   8f05f:	e8 0c 9f 0c 00       	call   158f70 <__wolf_rt_strbuf_new>
   8f064:	48 89 c3             	mov    %rax,%rbx
   8f067:	80 7c 24 28 2a       	cmpb   $0x2a,0x28(%rsp)
   8f06c:	74 76                	je     8f0e4 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x3c94>
   8f06e:	ba 13 00 00 00       	mov    $0x13,%edx
   8f073:	48 89 df             	mov    %rbx,%rdi
   8f076:	48 8d 35 7b 59 f8 ff 	lea    -0x7a685(%rip),%rsi        # 149f8 <__abi_tag+0x146fc>
   8f07d:	31 c9                	xor    %ecx,%ecx
   8f07f:	e8 9c 9f 0c 00       	call   159020 <__wolf_rt_strbuf_str>
   8f084:	4c 89 e7             	mov    %r12,%rdi
   8f087:	e8 14 ca 06 00       	call   fbaa0 <_Wfl.fl_type_name$425de879c2cc2a41>
   8f08c:	48 89 df             	mov    %rbx,%rdi
   8f08f:	48 89 c6             	mov    %rax,%rsi
   8f092:	31 c9                	xor    %ecx,%ecx
   8f094:	e8 87 9f 0c 00       	call   159020 <__wolf_rt_strbuf_str>
   8f099:	48 89 df             	mov    %rbx,%rdi
   8f09c:	48 8d 9c 24 90 03 00 	lea    0x390(%rsp),%rbx
   8f0a3:	00 
   8f0a4:	48 89 de             	mov    %rbx,%rsi
   8f0a7:	e8 74 9b 0c 00       	call   158c20 <__wolf_rt_strbuf_finish>
   8f0ac:	4c 8b 84 24 98 03 00 	mov    0x398(%rsp),%r8
   8f0b3:	00 
   8f0b4:	eb 74                	jmp    8f12a <_Wfl.fl_vm_exec$a5081fe92876eb37+0x3cda>
   8f0b6:	ba 04 00 00 00       	mov    $0x4,%edx
   8f0bb:	41 b8 19 00 00 00    	mov    $0x19,%r8d
   8f0c1:	4c 89 f7             	mov    %r14,%rdi
   8f0c4:	48 8d 35 25 f3 f8 ff 	lea    -0x70cdb(%rip),%rsi        # 1e3f0 <__abi_tag+0x1e0f4>
   8f0cb:	48 8d 0d b8 58 f8 ff 	lea    -0x7a748(%rip),%rcx        # 1498a <__abi_tag+0x1468e>
   8f0d2:	4c 8d 8c 24 78 1f 00 	lea    0x1f78(%rsp),%r9
   8f0d9:	00 
   8f0da:	e8 f1 39 08 00       	call   112ad0 <_Wfl.fl_raise_bytes.bytesview.4$e8b5fa06fc8adc2d>
   8f0df:	e9 33 fe ff ff       	jmp    8ef17 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x3ac7>
   8f0e4:	ba 0d 00 00 00       	mov    $0xd,%edx
   8f0e9:	48 89 df             	mov    %rbx,%rdi
   8f0ec:	48 8d 35 f8 58 f8 ff 	lea    -0x7a708(%rip),%rsi        # 149eb <__abi_tag+0x146ef>
   8f0f3:	31 c9                	xor    %ecx,%ecx
   8f0f5:	e8 26 9f 0c 00       	call   159020 <__wolf_rt_strbuf_str>
   8f0fa:	4c 89 e7             	mov    %r12,%rdi
   8f0fd:	e8 9e c9 06 00       	call   fbaa0 <_Wfl.fl_type_name$425de879c2cc2a41>
   8f102:	48 89 df             	mov    %rbx,%rdi
   8f105:	48 89 c6             	mov    %rax,%rsi
   8f108:	31 c9                	xor    %ecx,%ecx
   8f10a:	e8 11 9f 0c 00       	call   159020 <__wolf_rt_strbuf_str>
   8f10f:	48 89 df             	mov    %rbx,%rdi
   8f112:	48 8d 9c 24 a0 03 00 	lea    0x3a0(%rsp),%rbx
   8f119:	00 
   8f11a:	48 89 de             	mov    %rbx,%rsi
   8f11d:	e8 fe 9a 0c 00       	call   158c20 <__wolf_rt_strbuf_finish>
   8f122:	4c 8b 84 24 a8 03 00 	mov    0x3a8(%rsp),%r8
   8f129:	00 
   8f12a:	48 8b 0b             	mov    (%rbx),%rcx
   8f12d:	ba 04 00 00 00       	mov    $0x4,%edx
   8f132:	4c 89 f7             	mov    %r14,%rdi
   8f135:	48 8d 35 b4 f2 f8 ff 	lea    -0x70d4c(%rip),%rsi        # 1e3f0 <__abi_tag+0x1e0f4>
   8f13c:	4c 8d 8c 24 18 1f 00 	lea    0x1f18(%rsp),%r9
   8f143:	00 
   8f144:	e8 87 39 08 00       	call   112ad0 <_Wfl.fl_raise_bytes.bytesview.4$e8b5fa06fc8adc2d>
   8f149:	e9 c9 fd ff ff       	jmp    8ef17 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x3ac7>
   8f14e:	e8 1d 9e 0c 00       	call   158f70 <__wolf_rt_strbuf_new>
   8f153:	48 89 c3             	mov    %rax,%rbx
   8f156:	ba 06 00 00 00       	mov    $0x6,%edx
   8f15b:	48 89 c7             	mov    %rax,%rdi
   8f15e:	48 8d 35 3e 58 f8 ff 	lea    -0x7a7c2(%rip),%rsi        # 149a3 <__abi_tag+0x146a7>
   8f165:	31 c9                	xor    %ecx,%ecx
   8f167:	e8 b4 9e 0c 00       	call   159020 <__wolf_rt_strbuf_str>
   8f16c:	48 89 df             	mov    %rbx,%rdi
   8f16f:	48 89 ee             	mov    %rbp,%rsi
   8f172:	31 d2                	xor    %edx,%edx
   8f174:	e8 b7 9c 0c 00       	call   158e30 <__wolf_rt_strbuf_i64>
   8f179:	ba 13 00 00 00       	mov    $0x13,%edx
   8f17e:	48 89 df             	mov    %rbx,%rdi
   8f181:	48 8d 35 21 58 f8 ff 	lea    -0x7a7df(%rip),%rsi        # 149a9 <__abi_tag+0x146ad>
   8f188:	31 c9                	xor    %ecx,%ecx
   8f18a:	e8 91 9e 0c 00       	call   159020 <__wolf_rt_strbuf_str>
   8f18f:	48 89 df             	mov    %rbx,%rdi
   8f192:	4c 89 fe             	mov    %r15,%rsi
   8f195:	31 d2                	xor    %edx,%edx
   8f197:	e8 94 9c 0c 00       	call   158e30 <__wolf_rt_strbuf_i64>
   8f19c:	ba 01 00 00 00       	mov    $0x1,%edx
   8f1a1:	48 89 df             	mov    %rbx,%rdi
   8f1a4:	48 8d 35 13 54 f8 ff 	lea    -0x7abed(%rip),%rsi        # 145be <__abi_tag+0x142c2>
   8f1ab:	31 c9                	xor    %ecx,%ecx
   8f1ad:	e8 6e 9e 0c 00       	call   159020 <__wolf_rt_strbuf_str>
   8f1b2:	48 89 df             	mov    %rbx,%rdi
   8f1b5:	48 8d b4 24 e0 01 00 	lea    0x1e0(%rsp),%rsi
   8f1bc:	00 
   8f1bd:	e8 5e 9a 0c 00       	call   158c20 <__wolf_rt_strbuf_finish>
   8f1c2:	48 8b 8c 24 e0 01 00 	mov    0x1e0(%rsp),%rcx
   8f1c9:	00 
   8f1ca:	4c 8b 84 24 e8 01 00 	mov    0x1e8(%rsp),%r8
   8f1d1:	00 
   8f1d2:	ba 05 00 00 00       	mov    $0x5,%edx
   8f1d7:	4c 89 f7             	mov    %r14,%rdi
   8f1da:	48 8d 35 6e 55 f8 ff 	lea    -0x7aa92(%rip),%rsi        # 1474f <__abi_tag+0x14453>
   8f1e1:	4c 8d 8c 24 60 1f 00 	lea    0x1f60(%rsp),%r9
   8f1e8:	00 
   8f1e9:	e8 e2 38 08 00       	call   112ad0 <_Wfl.fl_raise_bytes.bytesview.4$e8b5fa06fc8adc2d>
   8f1ee:	e9 24 fd ff ff       	jmp    8ef17 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x3ac7>
   8f1f3:	49 83 ff 04          	cmp    $0x4,%r15
   8f1f7:	0f 87 12 09 00 00    	ja     8fb0f <_Wfl.fl_vm_exec$a5081fe92876eb37+0x46bf>
   8f1fd:	ba 16 00 00 00       	mov    $0x16,%edx
   8f202:	4c 0f a3 fa          	bt     %r15,%rdx
   8f206:	0f 83 03 09 00 00    	jae    8fb0f <_Wfl.fl_vm_exec$a5081fe92876eb37+0x46bf>
   8f20c:	48 8d 51 f4          	lea    -0xc(%rcx),%rdx
   8f210:	48 83 fa f8          	cmp    $0xfffffffffffffff8,%rdx
   8f214:	0f 82 c3 33 00 00    	jb     925dd <_Wfl.fl_vm_exec$a5081fe92876eb37+0x718d>
   8f21a:	48 85 c0             	test   %rax,%rax
   8f21d:	0f 88 ba 33 00 00    	js     925dd <_Wfl.fl_vm_exec$a5081fe92876eb37+0x718d>
   8f223:	41 89 c5             	mov    %eax,%r13d
   8f226:	48 8b 54 24 10       	mov    0x10(%rsp),%rdx
   8f22b:	4c 3b 6a 08          	cmp    0x8(%rdx),%r13
   8f22f:	0f 8d a8 33 00 00    	jge    925dd <_Wfl.fl_vm_exec$a5081fe92876eb37+0x718d>
   8f235:	48 8b 54 24 10       	mov    0x10(%rsp),%rdx
   8f23a:	48 8b 12             	mov    (%rdx),%rdx
   8f23d:	4d 69 e5 60 01 00 00 	imul   $0x160,%r13,%r12
   8f244:	4a 39 0c 22          	cmp    %rcx,(%rdx,%r12,1)
   8f248:	0f 85 8f 33 00 00    	jne    925dd <_Wfl.fl_vm_exec$a5081fe92876eb37+0x718d>
   8f24e:	48 c1 e8 20          	shr    $0x20,%rax
   8f252:	4a 39 44 22 19       	cmp    %rax,0x19(%rdx,%r12,1)
   8f257:	0f 85 80 33 00 00    	jne    925dd <_Wfl.fl_vm_exec$a5081fe92876eb37+0x718d>
   8f25d:	49 83 ff 01          	cmp    $0x1,%r15
   8f261:	0f 84 ec 06 00 00    	je     8f953 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4503>
   8f267:	49 83 ff 04          	cmp    $0x4,%r15
   8f26b:	0f 85 fd 06 00 00    	jne    8f96e <_Wfl.fl_vm_exec$a5081fe92876eb37+0x451e>
   8f271:	48 8d bc 24 c0 04 00 	lea    0x4c0(%rsp),%rdi
   8f278:	00 
   8f279:	48 8d b4 24 90 00 00 	lea    0x90(%rsp),%rsi
   8f280:	00 
   8f281:	e8 6a 79 03 00       	call   c6bf0 <_Wfl.fl_obj_index$6801bb8d311b3e64>
   8f286:	48 8b 4c 24 10       	mov    0x10(%rsp),%rcx
   8f28b:	48 3b 41 08          	cmp    0x8(%rcx),%rax
   8f28f:	0f 83 04 30 00 00    	jae    92299 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6e49>
   8f295:	48 8b 4c 24 10       	mov    0x10(%rsp),%rcx
   8f29a:	48 8b 09             	mov    (%rcx),%rcx
   8f29d:	48 69 c0 60 01 00 00 	imul   $0x160,%rax,%rax
   8f2a4:	48 8b 4c 01 11       	mov    0x11(%rcx,%rax,1),%rcx
   8f2a9:	e9 40 07 00 00       	jmp    8f9ee <_Wfl.fl_vm_exec$a5081fe92876eb37+0x459e>
   8f2ae:	48 3d 00 40 00 00    	cmp    $0x4000,%rax
   8f2b4:	0f 8d 95 f2 ff ff    	jge    8e54f <_Wfl.fl_vm_exec$a5081fe92876eb37+0x30ff>
   8f2ba:	48 3b 41 08          	cmp    0x8(%rcx),%rax
   8f2be:	0f 83 3d 32 00 00    	jae    92501 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x70b1>
   8f2c4:	48 8b 36             	mov    (%rsi),%rsi
   8f2c7:	48 8d 3c 6d 00 00 00 	lea    0x0(,%rbp,2),%rdi
   8f2ce:	00 
   8f2cf:	48 01 ef             	add    %rbp,%rdi
   8f2d2:	0f 10 04 fe          	movups (%rsi,%rdi,8),%xmm0
   8f2d6:	f2 0f 10 4c fe 10    	movsd  0x10(%rsi,%rdi,8),%xmm1
   8f2dc:	48 8b 09             	mov    (%rcx),%rcx
   8f2df:	0f 11 04 11          	movups %xmm0,(%rcx,%rdx,1)
   8f2e3:	f2 0f 11 4c 11 10    	movsd  %xmm1,0x10(%rcx,%rdx,1)
   8f2e9:	e9 1f fc ff ff       	jmp    8ef0d <_Wfl.fl_vm_exec$a5081fe92876eb37+0x3abd>
   8f2ee:	49 83 ff 01          	cmp    $0x1,%r15
   8f2f2:	0f 85 ff 08 00 00    	jne    8fbf7 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x47a7>
   8f2f8:	b8 6b ca eb 85       	mov    $0x85ebca6b,%eax
   8f2fd:	48 8d 88 4e af 4b 18 	lea    0x184baf4e(%rax),%rcx
   8f304:	48 83 7c 24 28 00    	cmpq   $0x0,0x28(%rsp)
   8f30a:	48 0f 44 c8          	cmove  %rax,%rcx
   8f30e:	e9 64 09 00 00       	jmp    8fc77 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4827>
   8f313:	4d 3b 6f 08          	cmp    0x8(%r15),%r13
   8f317:	0f 83 9a 30 00 00    	jae    923b7 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6f67>
   8f31d:	49 8b 0f             	mov    (%r15),%rcx
   8f320:	48 8b 44 24 18       	mov    0x18(%rsp),%rax
   8f325:	48 8b 44 01 18       	mov    0x18(%rcx,%rax,1),%rax
   8f32a:	48 89 c2             	mov    %rax,%rdx
   8f32d:	48 83 c2 02          	add    $0x2,%rdx
   8f331:	0f 80 e9 2e 00 00    	jo     92220 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6dd0>
   8f337:	48 8b 74 24 18       	mov    0x18(%rsp),%rsi
   8f33c:	48 89 54 31 18       	mov    %rdx,0x18(%rcx,%rsi,1)
   8f341:	48 8b 0c 24          	mov    (%rsp),%rcx
   8f345:	49 3b 4a 08          	cmp    0x8(%r10),%rcx
   8f349:	0f 83 1e 2f 00 00    	jae    9226d <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6e1d>
   8f34f:	49 8b 0a             	mov    (%r10),%rcx
   8f352:	48 8b 54 24 10       	mov    0x10(%rsp),%rdx
   8f357:	48 8b 4c 11 61       	mov    0x61(%rcx,%rdx,1),%rcx
   8f35c:	48 8b 49 08          	mov    0x8(%rcx),%rcx
   8f360:	48 39 c8             	cmp    %rcx,%rax
   8f363:	0f 83 04 2f 00 00    	jae    9226d <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6e1d>
   8f369:	48 ff c0             	inc    %rax
   8f36c:	48 39 c8             	cmp    %rcx,%rax
   8f36f:	0f 83 f8 2e 00 00    	jae    9226d <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6e1d>
   8f375:	ba 06 00 00 00       	mov    $0x6,%edx
   8f37a:	41 b8 0e 00 00 00    	mov    $0xe,%r8d
   8f380:	4c 89 f7             	mov    %r14,%rdi
   8f383:	48 8d 35 b8 56 f8 ff 	lea    -0x7a948(%rip),%rsi        # 14a42 <__abi_tag+0x14746>
   8f38a:	48 8d 0d b7 56 f8 ff 	lea    -0x7a949(%rip),%rcx        # 14a48 <__abi_tag+0x1474c>
   8f391:	4c 8d 8c 24 b8 1e 00 	lea    0x1eb8(%rsp),%r9
   8f398:	00 
   8f399:	e8 32 37 08 00       	call   112ad0 <_Wfl.fl_raise_bytes.bytesview.4$e8b5fa06fc8adc2d>
   8f39e:	e9 74 fb ff ff       	jmp    8ef17 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x3ac7>
   8f3a3:	4d 3b 6f 08          	cmp    0x8(%r15),%r13
   8f3a7:	0f 83 11 34 00 00    	jae    927be <_Wfl.fl_vm_exec$a5081fe92876eb37+0x736e>
   8f3ad:	49 8b 0f             	mov    (%r15),%rcx
   8f3b0:	48 8b 44 24 18       	mov    0x18(%rsp),%rax
   8f3b5:	48 8b 54 01 18       	mov    0x18(%rcx,%rax,1),%rdx
   8f3ba:	48 89 d0             	mov    %rdx,%rax
   8f3bd:	48 83 c0 02          	add    $0x2,%rax
   8f3c1:	0f 80 eb 2d 00 00    	jo     921b2 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6d62>
   8f3c7:	48 8b 74 24 18       	mov    0x18(%rsp),%rsi
   8f3cc:	48 89 44 31 18       	mov    %rax,0x18(%rcx,%rsi,1)
   8f3d1:	48 8b 34 24          	mov    (%rsp),%rsi
   8f3d5:	49 3b 72 08          	cmp    0x8(%r10),%rsi
   8f3d9:	0f 83 58 34 00 00    	jae    92837 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x73e7>
   8f3df:	49 8b 32             	mov    (%r10),%rsi
   8f3e2:	48 8b 7c 24 10       	mov    0x10(%rsp),%rdi
   8f3e7:	48 8b 7c 3e 61       	mov    0x61(%rsi,%rdi,1),%rdi
   8f3ec:	4c 8b 47 08          	mov    0x8(%rdi),%r8
   8f3f0:	4c 39 c2             	cmp    %r8,%rdx
   8f3f3:	0f 83 3e 34 00 00    	jae    92837 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x73e7>
   8f3f9:	4c 8d 4a 01          	lea    0x1(%rdx),%r9
   8f3fd:	4d 39 c1             	cmp    %r8,%r9
   8f400:	0f 83 31 34 00 00    	jae    92837 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x73e7>
   8f406:	49 89 c0             	mov    %rax,%r8
   8f409:	49 ff c0             	inc    %r8
   8f40c:	0f 80 e2 2d 00 00    	jo     921f4 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6da4>
   8f412:	48 8b 3f             	mov    (%rdi),%rdi
   8f415:	0f b7 14 17          	movzwl (%rdi,%rdx,1),%edx
   8f419:	48 8b 7c 24 18       	mov    0x18(%rsp),%rdi
   8f41e:	4c 89 44 39 18       	mov    %r8,0x18(%rcx,%rdi,1)
   8f423:	48 8b 4c 24 10       	mov    0x10(%rsp),%rcx
   8f428:	48 8b 4c 0e 61       	mov    0x61(%rsi,%rcx,1),%rcx
   8f42d:	48 3b 41 08          	cmp    0x8(%rcx),%rax
   8f431:	0f 83 9a 34 00 00    	jae    928d1 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x7481>
   8f437:	48 8b 7c 24 10       	mov    0x10(%rsp),%rdi
   8f43c:	48 8b 74 3e 29       	mov    0x29(%rsi,%rdi,1),%rsi
   8f441:	48 3b 56 08          	cmp    0x8(%rsi),%rdx
   8f445:	0f 83 bd 34 00 00    	jae    92908 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x74b8>
   8f44b:	48 8b 09             	mov    (%rcx),%rcx
   8f44e:	48 8b 36             	mov    (%rsi),%rsi
   8f451:	48 8d 14 52          	lea    (%rdx,%rdx,2),%rdx
   8f455:	48 8b 74 d6 08       	mov    0x8(%rsi,%rdx,8),%rsi
   8f45a:	31 d2                	xor    %edx,%edx
   8f45c:	80 3c 01 00          	cmpb   $0x0,(%rcx,%rax,1)
   8f460:	0f 95 c2             	setne  %dl
   8f463:	4c 89 f7             	mov    %r14,%rdi
   8f466:	48 8d 8c 24 68 02 00 	lea    0x268(%rsp),%rcx
   8f46d:	00 
   8f46e:	e8 bd 6a 06 00       	call   f5f30 <_Wfl.fl_import$c2bff798315c274a>
   8f473:	41 80 be 5c 01 00 00 	cmpb   $0x0,0x15c(%r14)
   8f47a:	00 
   8f47b:	0f 85 96 fa ff ff    	jne    8ef17 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x3ac7>
   8f481:	49 8b 86 ac 00 00 00 	mov    0xac(%r14),%rax
   8f488:	48 3d 00 40 00 00    	cmp    $0x4000,%rax
   8f48e:	0f 8d bb f0 ff ff    	jge    8e54f <_Wfl.fl_vm_exec$a5081fe92876eb37+0x30ff>
   8f494:	49 8b 8e a4 00 00 00 	mov    0xa4(%r14),%rcx
   8f49b:	48 3b 41 08          	cmp    0x8(%rcx),%rax
   8f49f:	0f 83 f9 2f 00 00    	jae    9249e <_Wfl.fl_vm_exec$a5081fe92876eb37+0x704e>
   8f4a5:	48 8b 09             	mov    (%rcx),%rcx
   8f4a8:	48 8d 14 40          	lea    (%rax,%rax,2),%rdx
   8f4ac:	48 8b b4 24 68 02 00 	mov    0x268(%rsp),%rsi
   8f4b3:	00 
   8f4b4:	48 89 34 d1          	mov    %rsi,(%rcx,%rdx,8)
   8f4b8:	48 8b b4 24 70 02 00 	mov    0x270(%rsp),%rsi
   8f4bf:	00 
   8f4c0:	48 89 74 d1 08       	mov    %rsi,0x8(%rcx,%rdx,8)
   8f4c5:	f2 0f 10 84 24 78 02 	movsd  0x278(%rsp),%xmm0
   8f4cc:	00 00 
   8f4ce:	e9 34 fa ff ff       	jmp    8ef07 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x3ab7>
   8f4d3:	b8 6b ca eb 85       	mov    $0x85ebca6b,%eax
   8f4d8:	48 05 93 35 14 7a    	add    $0x7a143593,%rax
   8f4de:	48 39 c5             	cmp    %rax,%rbp
   8f4e1:	7e 31                	jle    8f514 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x40c4>
   8f4e3:	ba 0c 00 00 00       	mov    $0xc,%edx
   8f4e8:	41 b9 33 00 00 00    	mov    $0x33,%r9d
   8f4ee:	4c 89 f7             	mov    %r14,%rdi
   8f4f1:	48 8d 35 48 53 f8 ff 	lea    -0x7acb8(%rip),%rsi        # 14840 <__abi_tag+0x14544>
   8f4f8:	48 8b 0c 24          	mov    (%rsp),%rcx
   8f4fc:	4c 8b 44 24 70       	mov    0x70(%rsp),%r8
   8f501:	6a 1a                	push   $0x1a
   8f503:	48 8d 05 4c 55 f8 ff 	lea    -0x7aab4(%rip),%rax        # 14a56 <__abi_tag+0x1475a>
   8f50a:	50                   	push   %rax
   8f50b:	e8 50 d7 06 00       	call   fcc60 <_Wfl.fl_vm_bug_at$494622488d4f7894>
   8f510:	48 83 c4 10          	add    $0x10,%rsp
   8f514:	49 8b 86 b5 01 00 00 	mov    0x1b5(%r14),%rax
   8f51b:	48 ff c0             	inc    %rax
   8f51e:	0f 81 f5 02 00 00    	jno    8f819 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x43c9>
   8f524:	e9 47 2c 00 00       	jmp    92170 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6d20>
   8f529:	ba 0c 00 00 00       	mov    $0xc,%edx
   8f52e:	4c 89 f7             	mov    %r14,%rdi
   8f531:	48 8d 35 08 53 f8 ff 	lea    -0x7acf8(%rip),%rsi        # 14840 <__abi_tag+0x14544>
   8f538:	4c 8b 24 24          	mov    (%rsp),%r12
   8f53c:	4c 89 e1             	mov    %r12,%rcx
   8f53f:	4c 8b 44 24 70       	mov    0x70(%rsp),%r8
   8f544:	4c 8b 4c 24 50       	mov    0x50(%rsp),%r9
   8f549:	6a 15                	push   $0x15
   8f54b:	48 8d 05 fa 52 f8 ff 	lea    -0x7ad06(%rip),%rax        # 1484c <__abi_tag+0x14550>
   8f552:	e9 61 0a 00 00       	jmp    8ffb8 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4b68>
   8f557:	4d 3b 6f 08          	cmp    0x8(%r15),%r13
   8f55b:	0f 83 89 32 00 00    	jae    927ea <_Wfl.fl_vm_exec$a5081fe92876eb37+0x739a>
   8f561:	49 8b 0f             	mov    (%r15),%rcx
   8f564:	48 8b 44 24 18       	mov    0x18(%rsp),%rax
   8f569:	48 8b 44 01 18       	mov    0x18(%rcx,%rax,1),%rax
   8f56e:	48 89 c2             	mov    %rax,%rdx
   8f571:	48 ff c2             	inc    %rdx
   8f574:	0f 80 4e 2c 00 00    	jo     921c8 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6d78>
   8f57a:	48 8b 74 24 18       	mov    0x18(%rsp),%rsi
   8f57f:	48 89 54 31 18       	mov    %rdx,0x18(%rcx,%rsi,1)
   8f584:	48 8b 0c 24          	mov    (%rsp),%rcx
   8f588:	49 3b 4a 08          	cmp    0x8(%r10),%rcx
   8f58c:	0f 83 d1 32 00 00    	jae    92863 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x7413>
   8f592:	49 8b 0a             	mov    (%r10),%rcx
   8f595:	48 8b 54 24 10       	mov    0x10(%rsp),%rdx
   8f59a:	48 8b 4c 11 61       	mov    0x61(%rcx,%rdx,1),%rcx
   8f59f:	48 3b 41 08          	cmp    0x8(%rcx),%rax
   8f5a3:	0f 83 ba 32 00 00    	jae    92863 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x7413>
   8f5a9:	48 8b 09             	mov    (%rcx),%rcx
   8f5ac:	0f b6 1c 01          	movzbl (%rcx,%rax,1),%ebx
   8f5b0:	48 83 7c 24 20 00    	cmpq   $0x0,0x20(%rsp)
   8f5b6:	7f 14                	jg     8f5cc <_Wfl.fl_vm_exec$a5081fe92876eb37+0x417c>
   8f5b8:	ba 16 00 00 00       	mov    $0x16,%edx
   8f5bd:	4c 89 f7             	mov    %r14,%rdi
   8f5c0:	48 8d 35 1c 53 f8 ff 	lea    -0x7ace4(%rip),%rsi        # 148e3 <__abi_tag+0x145e7>
   8f5c7:	e8 34 9e 02 00       	call   b9400 <_Wfl.fl_current_bug$22c9f442c8960c68>
   8f5cc:	49 8b 96 ac 00 00 00 	mov    0xac(%r14),%rdx
   8f5d3:	48 ff ca             	dec    %rdx
   8f5d6:	0f 80 02 2c 00 00    	jo     921de <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6d8e>
   8f5dc:	49 89 96 ac 00 00 00 	mov    %rdx,0xac(%r14)
   8f5e3:	49 8b 8e a4 00 00 00 	mov    0xa4(%r14),%rcx
   8f5ea:	48 8b 41 08          	mov    0x8(%rcx),%rax
   8f5ee:	48 39 c2             	cmp    %rax,%rdx
   8f5f1:	0f 83 2b 2f 00 00    	jae    92522 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x70d2>
   8f5f7:	48 8b 09             	mov    (%rcx),%rcx
   8f5fa:	48 8d 14 52          	lea    (%rdx,%rdx,2),%rdx
   8f5fe:	f2 0f 10 44 d1 10    	movsd  0x10(%rcx,%rdx,8),%xmm0
   8f604:	0f 10 0c d1          	movups (%rcx,%rdx,8),%xmm1
   8f608:	0f 29 8c 24 70 03 00 	movaps %xmm1,0x370(%rsp)
   8f60f:	00 
   8f610:	f2 0f 11 84 24 80 03 	movsd  %xmm0,0x380(%rsp)
   8f617:	00 00 
   8f619:	48 03 5c 24 68       	add    0x68(%rsp),%rbx
   8f61e:	0f 80 e6 2b 00 00    	jo     9220a <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6dba>
   8f624:	48 39 c3             	cmp    %rax,%rbx
   8f627:	0f 83 be 2e 00 00    	jae    924eb <_Wfl.fl_vm_exec$a5081fe92876eb37+0x709b>
   8f62d:	48 8d 04 5b          	lea    (%rbx,%rbx,2),%rax
   8f631:	f2 0f 10 44 c1 10    	movsd  0x10(%rcx,%rax,8),%xmm0
   8f637:	0f 10 0c c1          	movups (%rcx,%rax,8),%xmm1
   8f63b:	0f 29 8c 24 50 03 00 	movaps %xmm1,0x350(%rsp)
   8f642:	00 
   8f643:	f2 0f 11 84 24 60 03 	movsd  %xmm0,0x360(%rsp)
   8f64a:	00 00 
   8f64c:	4c 89 f7             	mov    %r14,%rdi
   8f64f:	48 8d b4 24 50 03 00 	lea    0x350(%rsp),%rsi
   8f656:	00 
   8f657:	48 8d 94 24 70 03 00 	lea    0x370(%rsp),%rdx
   8f65e:	00 
   8f65f:	e8 1c fd 02 00       	call   bf380 <_Wfl.fl_list_push$255051f5e64d80eb>
   8f664:	e9 ae f8 ff ff       	jmp    8ef17 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x3ac7>
   8f669:	48 ff 4c 24 20       	decq   0x20(%rsp)
   8f66e:	0f 80 12 2b 00 00    	jo     92186 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6d36>
   8f674:	48 8b 44 24 20       	mov    0x20(%rsp),%rax
   8f679:	48 8b 4c 24 38       	mov    0x38(%rsp),%rcx
   8f67e:	48 3b 41 08          	cmp    0x8(%rcx),%rax
   8f682:	0f 83 15 31 00 00    	jae    9279d <_Wfl.fl_vm_exec$a5081fe92876eb37+0x734d>
   8f688:	48 8b 44 24 38       	mov    0x38(%rsp),%rax
   8f68d:	48 8b 00             	mov    (%rax),%rax
   8f690:	48 8b 4c 24 20       	mov    0x20(%rsp),%rcx
   8f695:	48 8d 0c 49          	lea    (%rcx,%rcx,2),%rcx
   8f699:	31 d2                	xor    %edx,%edx
   8f69b:	48 83 3c c8 00       	cmpq   $0x0,(%rax,%rcx,8)
   8f6a0:	0f 95 c2             	setne  %dl
   8f6a3:	48 c7 04 c8 01 00 00 	movq   $0x1,(%rax,%rcx,8)
   8f6aa:	00 
   8f6ab:	48 89 54 c8 08       	mov    %rdx,0x8(%rax,%rcx,8)
   8f6b0:	48 c7 44 c8 10 00 00 	movq   $0x0,0x10(%rax,%rcx,8)
   8f6b7:	00 00 
   8f6b9:	e9 59 f8 ff ff       	jmp    8ef17 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x3ac7>
   8f6be:	4d 3b 6f 08          	cmp    0x8(%r15),%r13
   8f6c2:	0f 83 4c 2f 00 00    	jae    92614 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x71c4>
   8f6c8:	49 8b 0f             	mov    (%r15),%rcx
   8f6cb:	48 8b 44 24 18       	mov    0x18(%rsp),%rax
   8f6d0:	48 8b 44 01 18       	mov    0x18(%rcx,%rax,1),%rax
   8f6d5:	48 89 c2             	mov    %rax,%rdx
   8f6d8:	48 83 c2 02          	add    $0x2,%rdx
   8f6dc:	0f 80 78 2a 00 00    	jo     9215a <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6d0a>
   8f6e2:	48 8b 74 24 18       	mov    0x18(%rsp),%rsi
   8f6e7:	48 89 54 31 18       	mov    %rdx,0x18(%rcx,%rsi,1)
   8f6ec:	48 8b 0c 24          	mov    (%rsp),%rcx
   8f6f0:	49 3b 4a 08          	cmp    0x8(%r10),%rcx
   8f6f4:	0f 83 bf 2f 00 00    	jae    926b9 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x7269>
   8f6fa:	49 8b 0a             	mov    (%r10),%rcx
   8f6fd:	48 8b 54 24 10       	mov    0x10(%rsp),%rdx
   8f702:	48 8b 4c 11 61       	mov    0x61(%rcx,%rdx,1),%rcx
   8f707:	48 8b 51 08          	mov    0x8(%rcx),%rdx
   8f70b:	48 39 d0             	cmp    %rdx,%rax
   8f70e:	0f 83 a5 2f 00 00    	jae    926b9 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x7269>
   8f714:	48 8d 70 01          	lea    0x1(%rax),%rsi
   8f718:	48 39 d6             	cmp    %rdx,%rsi
   8f71b:	0f 83 98 2f 00 00    	jae    926b9 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x7269>
   8f721:	48 89 df             	mov    %rbx,%rdi
   8f724:	48 8b 09             	mov    (%rcx),%rcx
   8f727:	44 0f b7 3c 01       	movzwl (%rcx,%rax,1),%r15d
   8f72c:	48 8b 47 08          	mov    0x8(%rdi),%rax
   8f730:	48 3b 47 10          	cmp    0x10(%rdi),%rax
   8f734:	0f 83 54 06 00 00    	jae    8fd8e <_Wfl.fl_vm_exec$a5081fe92876eb37+0x493e>
   8f73a:	48 8b 0f             	mov    (%rdi),%rcx
   8f73d:	4c 89 3c c1          	mov    %r15,(%rcx,%rax,8)
   8f741:	48 ff c0             	inc    %rax
   8f744:	48 89 47 08          	mov    %rax,0x8(%rdi)
   8f748:	e9 56 06 00 00       	jmp    8fda3 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4953>
   8f74d:	48 8b 54 24 20       	mov    0x20(%rsp),%rdx
   8f752:	48 ff ca             	dec    %rdx
   8f755:	0f 80 41 2a 00 00    	jo     9219c <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6d4c>
   8f75b:	48 8b 44 24 38       	mov    0x38(%rsp),%rax
   8f760:	48 8b 48 08          	mov    0x8(%rax),%rcx
   8f764:	48 39 ca             	cmp    %rcx,%rdx
   8f767:	0f 83 62 2f 00 00    	jae    926cf <_Wfl.fl_vm_exec$a5081fe92876eb37+0x727f>
   8f76d:	48 8b 44 24 38       	mov    0x38(%rsp),%rax
   8f772:	48 8b 00             	mov    (%rax),%rax
   8f775:	48 8d 14 52          	lea    (%rdx,%rdx,2),%rdx
   8f779:	48 8b 34 d0          	mov    (%rax,%rdx,8),%rsi
   8f77d:	48 8d 7e fb          	lea    -0x5(%rsi),%rdi
   8f781:	48 83 ff 01          	cmp    $0x1,%rdi
   8f785:	0f 87 67 05 00 00    	ja     8fcf2 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x48a2>
   8f78b:	48 8b 54 d0 08       	mov    0x8(%rax,%rdx,8),%rdx
   8f790:	48 85 d2             	test   %rdx,%rdx
   8f793:	0f 88 85 31 00 00    	js     9291e <_Wfl.fl_vm_exec$a5081fe92876eb37+0x74ce>
   8f799:	41 89 d0             	mov    %edx,%r8d
   8f79c:	4d 3b 42 08          	cmp    0x8(%r10),%r8
   8f7a0:	0f 8d 78 31 00 00    	jge    9291e <_Wfl.fl_vm_exec$a5081fe92876eb37+0x74ce>
   8f7a6:	49 8b 3a             	mov    (%r10),%rdi
   8f7a9:	4d 69 c0 60 01 00 00 	imul   $0x160,%r8,%r8
   8f7b0:	4a 39 34 07          	cmp    %rsi,(%rdi,%r8,1)
   8f7b4:	0f 85 64 31 00 00    	jne    9291e <_Wfl.fl_vm_exec$a5081fe92876eb37+0x74ce>
   8f7ba:	48 c1 ea 20          	shr    $0x20,%rdx
   8f7be:	4a 39 54 07 19       	cmp    %rdx,0x19(%rdi,%r8,1)
   8f7c3:	0f 85 55 31 00 00    	jne    9291e <_Wfl.fl_vm_exec$a5081fe92876eb37+0x74ce>
   8f7c9:	4a 8b 5c 07 59       	mov    0x59(%rdi,%r8,1),%rbx
   8f7ce:	e9 21 05 00 00       	jmp    8fcf4 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x48a4>
   8f7d3:	48 85 ed             	test   %rbp,%rbp
   8f7d6:	7f 31                	jg     8f809 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x43b9>
   8f7d8:	ba 0c 00 00 00       	mov    $0xc,%edx
   8f7dd:	41 b9 34 00 00 00    	mov    $0x34,%r9d
   8f7e3:	4c 89 f7             	mov    %r14,%rdi
   8f7e6:	48 8d 35 53 50 f8 ff 	lea    -0x7afad(%rip),%rsi        # 14840 <__abi_tag+0x14544>
   8f7ed:	48 8b 0c 24          	mov    (%rsp),%rcx
   8f7f1:	4c 8b 44 24 70       	mov    0x70(%rsp),%r8
   8f7f6:	6a 1b                	push   $0x1b
   8f7f8:	48 8d 05 71 52 f8 ff 	lea    -0x7ad8f(%rip),%rax        # 14a70 <__abi_tag+0x14774>
   8f7ff:	50                   	push   %rax
   8f800:	e8 5b d4 06 00       	call   fcc60 <_Wfl.fl_vm_bug_at$494622488d4f7894>
   8f805:	48 83 c4 10          	add    $0x10,%rsp
   8f809:	49 8b 86 b5 01 00 00 	mov    0x1b5(%r14),%rax
   8f810:	48 ff c8             	dec    %rax
   8f813:	0f 80 15 29 00 00    	jo     9212e <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6cde>
   8f819:	49 89 86 b5 01 00 00 	mov    %rax,0x1b5(%r14)
   8f820:	e9 f2 f6 ff ff       	jmp    8ef17 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x3ac7>
   8f825:	48 83 7c 24 20 00    	cmpq   $0x0,0x20(%rsp)
   8f82b:	7f 14                	jg     8f841 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x43f1>
   8f82d:	ba 16 00 00 00       	mov    $0x16,%edx
   8f832:	4c 89 f7             	mov    %r14,%rdi
   8f835:	48 8d 35 a7 50 f8 ff 	lea    -0x7af59(%rip),%rsi        # 148e3 <__abi_tag+0x145e7>
   8f83c:	e8 bf 9b 02 00       	call   b9400 <_Wfl.fl_current_bug$22c9f442c8960c68>
   8f841:	49 8b 86 ac 00 00 00 	mov    0xac(%r14),%rax
   8f848:	48 ff c8             	dec    %rax
   8f84b:	0f 80 bc 28 00 00    	jo     9210d <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6cbd>
   8f851:	49 89 86 ac 00 00 00 	mov    %rax,0xac(%r14)
   8f858:	49 8b 8e a4 00 00 00 	mov    0xa4(%r14),%rcx
   8f85f:	48 3b 41 08          	cmp    0x8(%rcx),%rax
   8f863:	0f 83 c1 2d 00 00    	jae    9262a <_Wfl.fl_vm_exec$a5081fe92876eb37+0x71da>
   8f869:	48 8b 09             	mov    (%rcx),%rcx
   8f86c:	48 8d 04 40          	lea    (%rax,%rax,2),%rax
   8f870:	f2 0f 10 44 c1 10    	movsd  0x10(%rcx,%rax,8),%xmm0
   8f876:	0f 10 0c c1          	movups (%rcx,%rax,8),%xmm1
   8f87a:	41 0f 11 8e 5d 01 00 	movups %xmm1,0x15d(%r14)
   8f881:	00 
   8f882:	f2 41 0f 11 86 6d 01 	movsd  %xmm0,0x16d(%r14)
   8f889:	00 00 
   8f88b:	41 c6 86 5c 01 00 00 	movb   $0x1,0x15c(%r14)
   8f892:	01 
   8f893:	e9 7f f6 ff ff       	jmp    8ef17 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x3ac7>
   8f898:	49 8b 47 08          	mov    0x8(%r15),%rax
   8f89c:	49 39 c5             	cmp    %rax,%r13
   8f89f:	0f 83 de 29 00 00    	jae    92283 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6e33>
   8f8a5:	49 8b 37             	mov    (%r15),%rsi
   8f8a8:	48 8b 4c 24 18       	mov    0x18(%rsp),%rcx
   8f8ad:	48 8b 54 0e 18       	mov    0x18(%rsi,%rcx,1),%rdx
   8f8b2:	48 89 d1             	mov    %rdx,%rcx
   8f8b5:	48 83 c1 02          	add    $0x2,%rcx
   8f8b9:	0f 80 38 28 00 00    	jo     920f7 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6ca7>
   8f8bf:	48 8b 7c 24 18       	mov    0x18(%rsp),%rdi
   8f8c4:	48 89 4c 3e 18       	mov    %rcx,0x18(%rsi,%rdi,1)
   8f8c9:	48 8b 34 24          	mov    (%rsp),%rsi
   8f8cd:	49 3b 72 08          	cmp    0x8(%r10),%rsi
   8f8d1:	0f 83 3b 2a 00 00    	jae    92312 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6ec2>
   8f8d7:	49 8b 32             	mov    (%r10),%rsi
   8f8da:	48 8b 7c 24 10       	mov    0x10(%rsp),%rdi
   8f8df:	48 8b 7c 3e 61       	mov    0x61(%rsi,%rdi,1),%rdi
   8f8e4:	48 8b 77 08          	mov    0x8(%rdi),%rsi
   8f8e8:	48 39 f2             	cmp    %rsi,%rdx
   8f8eb:	0f 83 21 2a 00 00    	jae    92312 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6ec2>
   8f8f1:	4c 8d 42 01          	lea    0x1(%rdx),%r8
   8f8f5:	49 39 f0             	cmp    %rsi,%r8
   8f8f8:	0f 83 14 2a 00 00    	jae    92312 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6ec2>
   8f8fe:	49 8b 71 08          	mov    0x8(%r9),%rsi
   8f902:	48 83 fe 40          	cmp    $0x40,%rsi
   8f906:	0f 8c a4 04 00 00    	jl     8fdb0 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4960>
   8f90c:	ba 05 00 00 00       	mov    $0x5,%edx
   8f911:	41 b8 1a 00 00 00    	mov    $0x1a,%r8d
   8f917:	4c 89 f7             	mov    %r14,%rdi
   8f91a:	48 8d 35 db 4e f8 ff 	lea    -0x7b125(%rip),%rsi        # 147fc <__abi_tag+0x14500>
   8f921:	48 8d 0d 63 51 f8 ff 	lea    -0x7ae9d(%rip),%rcx        # 14a8b <__abi_tag+0x1478f>
   8f928:	4c 8d 8c 24 a0 1e 00 	lea    0x1ea0(%rsp),%r9
   8f92f:	00 
   8f930:	e8 9b 31 08 00       	call   112ad0 <_Wfl.fl_raise_bytes.bytesview.4$e8b5fa06fc8adc2d>
   8f935:	e9 dd f5 ff ff       	jmp    8ef17 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x3ac7>
   8f93a:	49 8b 41 08          	mov    0x8(%r9),%rax
   8f93e:	48 85 c0             	test   %rax,%rax
   8f941:	0f 8e ad 04 00 00    	jle    8fdf4 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x49a4>
   8f947:	48 ff c8             	dec    %rax
   8f94a:	49 89 41 08          	mov    %rax,0x8(%r9)
   8f94e:	e9 c4 f5 ff ff       	jmp    8ef17 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x3ac7>
   8f953:	b8 6b ca eb 85       	mov    $0x85ebca6b,%eax
   8f958:	48 8d 88 4e af 4b 18 	lea    0x184baf4e(%rax),%rcx
   8f95f:	48 83 7c 24 28 00    	cmpq   $0x0,0x28(%rsp)
   8f965:	48 0f 44 c8          	cmove  %rax,%rcx
   8f969:	e9 80 00 00 00       	jmp    8f9ee <_Wfl.fl_vm_exec$a5081fe92876eb37+0x459e>
   8f96e:	48 8b 54 24 28       	mov    0x28(%rsp),%rdx
   8f973:	0f b6 c2             	movzbl %dl,%eax
   8f976:	35 c5 9d 1c 81       	xor    $0x811c9dc5,%eax
   8f97b:	69 c0 93 01 00 01    	imul   $0x1000193,%eax,%eax
   8f981:	0f b6 ce             	movzbl %dh,%ecx
   8f984:	31 c1                	xor    %eax,%ecx
   8f986:	69 c1 93 01 00 01    	imul   $0x1000193,%ecx,%eax
   8f98c:	89 d1                	mov    %edx,%ecx
   8f98e:	c1 e9 10             	shr    $0x10,%ecx
   8f991:	0f b6 c9             	movzbl %cl,%ecx
   8f994:	31 c1                	xor    %eax,%ecx
   8f996:	69 c1 93 01 00 01    	imul   $0x1000193,%ecx,%eax
   8f99c:	89 d1                	mov    %edx,%ecx
   8f99e:	c1 e9 18             	shr    $0x18,%ecx
   8f9a1:	31 c1                	xor    %eax,%ecx
   8f9a3:	69 c1 93 01 00 01    	imul   $0x1000193,%ecx,%eax
   8f9a9:	48 89 d1             	mov    %rdx,%rcx
   8f9ac:	48 c1 e9 20          	shr    $0x20,%rcx
   8f9b0:	0f b6 c9             	movzbl %cl,%ecx
   8f9b3:	31 c1                	xor    %eax,%ecx
   8f9b5:	69 c1 93 01 00 01    	imul   $0x1000193,%ecx,%eax
   8f9bb:	48 89 d1             	mov    %rdx,%rcx
   8f9be:	48 c1 e9 28          	shr    $0x28,%rcx
   8f9c2:	0f b6 c9             	movzbl %cl,%ecx
   8f9c5:	31 c1                	xor    %eax,%ecx
   8f9c7:	69 c1 93 01 00 01    	imul   $0x1000193,%ecx,%eax
   8f9cd:	48 89 d1             	mov    %rdx,%rcx
   8f9d0:	48 c1 e9 30          	shr    $0x30,%rcx
   8f9d4:	0f b6 c9             	movzbl %cl,%ecx
   8f9d7:	31 c1                	xor    %eax,%ecx
   8f9d9:	69 c1 93 01 00 01    	imul   $0x1000193,%ecx,%eax
   8f9df:	48 89 d1             	mov    %rdx,%rcx
   8f9e2:	48 c1 e9 38          	shr    $0x38,%rcx
   8f9e6:	31 c1                	xor    %eax,%ecx
   8f9e8:	69 c9 93 01 00 01    	imul   $0x1000193,%ecx,%ecx
   8f9ee:	48 8d bc 24 c0 04 00 	lea    0x4c0(%rsp),%rdi
   8f9f5:	00 
   8f9f6:	4c 89 ee             	mov    %r13,%rsi
   8f9f9:	48 8d 94 24 90 00 00 	lea    0x90(%rsp),%rdx
   8fa00:	00 
   8fa01:	e8 ea 74 03 00       	call   c6ef0 <_Wfl.fl_map_probe$234ad4c981fd6647>
   8fa06:	84 d2                	test   %dl,%dl
   8fa08:	0f 84 01 01 00 00    	je     8fb0f <_Wfl.fl_vm_exec$a5081fe92876eb37+0x46bf>
   8fa0e:	48 8b 4c 24 10       	mov    0x10(%rsp),%rcx
   8fa13:	4c 3b 69 08          	cmp    0x8(%rcx),%r13
   8fa17:	0f 83 7c 28 00 00    	jae    92299 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6e49>
   8fa1d:	48 8b 4c 24 10       	mov    0x10(%rsp),%rcx
   8fa22:	48 8b 09             	mov    (%rcx),%rcx
   8fa25:	4a 8b 54 21 39       	mov    0x39(%rcx,%r12,1),%rdx
   8fa2a:	48 3b 42 08          	cmp    0x8(%rdx),%rax
   8fa2e:	0f 83 65 28 00 00    	jae    92299 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6e49>
   8fa34:	48 8b 12             	mov    (%rdx),%rdx
   8fa37:	48 8b 04 c2          	mov    (%rdx,%rax,8),%rax
   8fa3b:	48 ff c8             	dec    %rax
   8fa3e:	0f 80 45 26 00 00    	jo     92089 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6c39>
   8fa44:	4a 8b 4c 21 31       	mov    0x31(%rcx,%r12,1),%rcx
   8fa49:	48 3b 41 08          	cmp    0x8(%rcx),%rax
   8fa4d:	0f 83 46 28 00 00    	jae    92299 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6e49>
   8fa53:	48 83 7c 24 20 00    	cmpq   $0x0,0x20(%rsp)
   8fa59:	7f 14                	jg     8fa6f <_Wfl.fl_vm_exec$a5081fe92876eb37+0x461f>
   8fa5b:	ba 16 00 00 00       	mov    $0x16,%edx
   8fa60:	4c 89 f7             	mov    %r14,%rdi
   8fa63:	48 8d 35 79 4e f8 ff 	lea    -0x7b187(%rip),%rsi        # 148e3 <__abi_tag+0x145e7>
   8fa6a:	e8 91 99 02 00       	call   b9400 <_Wfl.fl_current_bug$22c9f442c8960c68>
   8fa6f:	49 8b 86 ac 00 00 00 	mov    0xac(%r14),%rax
   8fa76:	48 ff c8             	dec    %rax
   8fa79:	0f 80 cb 24 00 00    	jo     91f4a <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6afa>
   8fa7f:	49 89 86 ac 00 00 00 	mov    %rax,0xac(%r14)
   8fa86:	49 8b 8e a4 00 00 00 	mov    0xa4(%r14),%rcx
   8fa8d:	48 3b 41 08          	cmp    0x8(%rcx),%rax
   8fa91:	0f 83 91 28 00 00    	jae    92328 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6ed8>
   8fa97:	48 8b 09             	mov    (%rcx),%rcx
   8fa9a:	48 8d 04 40          	lea    (%rax,%rax,2),%rax
   8fa9e:	f2 0f 10 44 c1 10    	movsd  0x10(%rcx,%rax,8),%xmm0
   8faa4:	0f 10 0c c1          	movups (%rcx,%rax,8),%xmm1
   8faa8:	0f 29 8c 24 90 04 00 	movaps %xmm1,0x490(%rsp)
   8faaf:	00 
   8fab0:	f2 0f 11 84 24 a0 04 	movsd  %xmm0,0x4a0(%rsp)
   8fab7:	00 00 
   8fab9:	4c 89 f7             	mov    %r14,%rdi
   8fabc:	48 8d b4 24 98 01 00 	lea    0x198(%rsp),%rsi
   8fac3:	00 
   8fac4:	48 8d 94 24 90 00 00 	lea    0x90(%rsp),%rdx
   8facb:	00 
   8facc:	48 8d 8c 24 90 04 00 	lea    0x490(%rsp),%rcx
   8fad3:	00 
   8fad4:	e8 d7 fa 02 00       	call   bf5b0 <_Wfl.fl_map_set$540fcc92758450a0>
   8fad9:	84 c0                	test   %al,%al
   8fadb:	0f 85 36 f4 ff ff    	jne    8ef17 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x3ac7>
   8fae1:	ba 0c 00 00 00       	mov    $0xc,%edx
   8fae6:	4c 89 f7             	mov    %r14,%rdi
   8fae9:	48 8d 35 50 4d f8 ff 	lea    -0x7b2b0(%rip),%rsi        # 14840 <__abi_tag+0x14544>
   8faf0:	4c 8b 24 24          	mov    (%rsp),%r12
   8faf4:	4c 89 e1             	mov    %r12,%rcx
   8faf7:	4c 8b 44 24 70       	mov    0x70(%rsp),%r8
   8fafc:	4c 8b 4c 24 50       	mov    0x50(%rsp),%r9
   8fb01:	6a 1e                	push   $0x1e
   8fb03:	48 8d 05 3d 4e f8 ff 	lea    -0x7b1c3(%rip),%rax        # 14947 <__abi_tag+0x1464b>
   8fb0a:	e9 a9 04 00 00       	jmp    8ffb8 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4b68>
   8fb0f:	48 8b 44 24 28       	mov    0x28(%rsp),%rax
   8fb14:	48 3b 43 08          	cmp    0x8(%rbx),%rax
   8fb18:	0f 83 c7 2b 00 00    	jae    926e5 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x7295>
   8fb1e:	48 8b 03             	mov    (%rbx),%rax
   8fb21:	48 8b 4c 24 28       	mov    0x28(%rsp),%rcx
   8fb26:	48 8b 3c c8          	mov    (%rax,%rcx,8),%rdi
   8fb2a:	48 8d b4 24 20 02 00 	lea    0x220(%rsp),%rsi
   8fb31:	00 
   8fb32:	e8 79 8c 0c 00       	call   1587b0 <__wolf_rt_str_from_utf8>
   8fb37:	48 85 c0             	test   %rax,%rax
   8fb3a:	0f 85 c2 14 00 00    	jne    91002 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5bb2>
   8fb40:	4c 8b bc 24 20 02 00 	mov    0x220(%rsp),%r15
   8fb47:	00 
   8fb48:	48 8b 9c 24 28 02 00 	mov    0x228(%rsp),%rbx
   8fb4f:	00 
   8fb50:	e8 1b 94 0c 00       	call   158f70 <__wolf_rt_strbuf_new>
   8fb55:	49 89 c4             	mov    %rax,%r12
   8fb58:	ba 10 00 00 00       	mov    $0x10,%edx
   8fb5d:	48 89 c7             	mov    %rax,%rdi
   8fb60:	48 8d 35 69 de f8 ff 	lea    -0x72197(%rip),%rsi        # 1d9d0 <__abi_tag+0x1d6d4>
   8fb67:	31 c9                	xor    %ecx,%ecx
   8fb69:	e8 b2 94 0c 00       	call   159020 <__wolf_rt_strbuf_str>
   8fb6e:	4c 89 e7             	mov    %r12,%rdi
   8fb71:	4c 89 fe             	mov    %r15,%rsi
   8fb74:	48 89 da             	mov    %rbx,%rdx
   8fb77:	31 c9                	xor    %ecx,%ecx
   8fb79:	e8 a2 94 0c 00       	call   159020 <__wolf_rt_strbuf_str>
   8fb7e:	ba 01 00 00 00       	mov    $0x1,%edx
   8fb83:	4c 89 e7             	mov    %r12,%rdi
   8fb86:	48 8d 35 b9 4d f8 ff 	lea    -0x7b247(%rip),%rsi        # 14946 <__abi_tag+0x1464a>
   8fb8d:	31 c9                	xor    %ecx,%ecx
   8fb8f:	e8 8c 94 0c 00       	call   159020 <__wolf_rt_strbuf_str>
   8fb94:	4c 89 e7             	mov    %r12,%rdi
   8fb97:	48 8d b4 24 10 02 00 	lea    0x210(%rsp),%rsi
   8fb9e:	00 
   8fb9f:	e8 7c 90 0c 00       	call   158c20 <__wolf_rt_strbuf_finish>
   8fba4:	48 8b 8c 24 10 02 00 	mov    0x210(%rsp),%rcx
   8fbab:	00 
   8fbac:	4c 8b 84 24 18 02 00 	mov    0x218(%rsp),%r8
   8fbb3:	00 
   8fbb4:	49 81 f8 80 00 00 00 	cmp    $0x80,%r8
   8fbbb:	7c 19                	jl     8fbd6 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4786>
   8fbbd:	80 39 c0             	cmpb   $0xc0,(%rcx)
   8fbc0:	0f 8c 1f 2b 00 00    	jl     926e5 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x7295>
   8fbc6:	41 b8 7f 00 00 00    	mov    $0x7f,%r8d
   8fbcc:	80 79 7f bf          	cmpb   $0xbf,0x7f(%rcx)
   8fbd0:	0f 8e 0f 2b 00 00    	jle    926e5 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x7295>
   8fbd6:	ba 04 00 00 00       	mov    $0x4,%edx
   8fbdb:	4c 89 f7             	mov    %r14,%rdi
   8fbde:	48 8d 35 eb e7 f8 ff 	lea    -0x71815(%rip),%rsi        # 1e3d0 <__abi_tag+0x1e0d4>
   8fbe5:	4c 8d 8c 24 c0 1f 00 	lea    0x1fc0(%rsp),%r9
   8fbec:	00 
   8fbed:	e8 de 2e 08 00       	call   112ad0 <_Wfl.fl_raise_bytes.bytesview.4$e8b5fa06fc8adc2d>
   8fbf2:	e9 20 f3 ff ff       	jmp    8ef17 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x3ac7>
   8fbf7:	48 8b 54 24 28       	mov    0x28(%rsp),%rdx
   8fbfc:	0f b6 c2             	movzbl %dl,%eax
   8fbff:	35 c5 9d 1c 81       	xor    $0x811c9dc5,%eax
   8fc04:	69 c0 93 01 00 01    	imul   $0x1000193,%eax,%eax
   8fc0a:	0f b6 ce             	movzbl %dh,%ecx
   8fc0d:	31 c1                	xor    %eax,%ecx
   8fc0f:	69 c1 93 01 00 01    	imul   $0x1000193,%ecx,%eax
   8fc15:	89 d1                	mov    %edx,%ecx
   8fc17:	c1 e9 10             	shr    $0x10,%ecx
   8fc1a:	0f b6 c9             	movzbl %cl,%ecx
   8fc1d:	31 c1                	xor    %eax,%ecx
   8fc1f:	69 c1 93 01 00 01    	imul   $0x1000193,%ecx,%eax
   8fc25:	89 d1                	mov    %edx,%ecx
   8fc27:	c1 e9 18             	shr    $0x18,%ecx
   8fc2a:	31 c1                	xor    %eax,%ecx
   8fc2c:	69 c1 93 01 00 01    	imul   $0x1000193,%ecx,%eax
   8fc32:	48 89 d1             	mov    %rdx,%rcx
   8fc35:	48 c1 e9 20          	shr    $0x20,%rcx
   8fc39:	0f b6 c9             	movzbl %cl,%ecx
   8fc3c:	31 c1                	xor    %eax,%ecx
   8fc3e:	69 c1 93 01 00 01    	imul   $0x1000193,%ecx,%eax
   8fc44:	48 89 d1             	mov    %rdx,%rcx
   8fc47:	48 c1 e9 28          	shr    $0x28,%rcx
   8fc4b:	0f b6 c9             	movzbl %cl,%ecx
   8fc4e:	31 c1                	xor    %eax,%ecx
   8fc50:	69 c1 93 01 00 01    	imul   $0x1000193,%ecx,%eax
   8fc56:	48 89 d1             	mov    %rdx,%rcx
   8fc59:	48 c1 e9 30          	shr    $0x30,%rcx
   8fc5d:	0f b6 c9             	movzbl %cl,%ecx
   8fc60:	31 c1                	xor    %eax,%ecx
   8fc62:	69 c1 93 01 00 01    	imul   $0x1000193,%ecx,%eax
   8fc68:	48 89 d1             	mov    %rdx,%rcx
   8fc6b:	48 c1 e9 38          	shr    $0x38,%rcx
   8fc6f:	31 c1                	xor    %eax,%ecx
   8fc71:	69 c9 93 01 00 01    	imul   $0x1000193,%ecx,%ecx
   8fc77:	48 8d bc 24 c0 04 00 	lea    0x4c0(%rsp),%rdi
   8fc7e:	00 
   8fc7f:	48 89 de             	mov    %rbx,%rsi
   8fc82:	48 8d 94 24 90 00 00 	lea    0x90(%rsp),%rdx
   8fc89:	00 
   8fc8a:	e8 61 72 03 00       	call   c6ef0 <_Wfl.fl_map_probe$234ad4c981fd6647>
   8fc8f:	84 d2                	test   %dl,%dl
   8fc91:	0f 84 a2 0a 00 00    	je     90739 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x52e9>
   8fc97:	48 8b 4c 24 10       	mov    0x10(%rsp),%rcx
   8fc9c:	48 3b 59 08          	cmp    0x8(%rcx),%rbx
   8fca0:	0f 83 76 2a 00 00    	jae    9271c <_Wfl.fl_vm_exec$a5081fe92876eb37+0x72cc>
   8fca6:	48 8b 4c 24 10       	mov    0x10(%rsp),%rcx
   8fcab:	48 8b 09             	mov    (%rcx),%rcx
   8fcae:	48 8b 54 29 39       	mov    0x39(%rcx,%rbp,1),%rdx
   8fcb3:	48 3b 42 08          	cmp    0x8(%rdx),%rax
   8fcb7:	0f 83 5f 2a 00 00    	jae    9271c <_Wfl.fl_vm_exec$a5081fe92876eb37+0x72cc>
   8fcbd:	48 8b 12             	mov    (%rdx),%rdx
   8fcc0:	48 8b 04 c2          	mov    (%rdx,%rax,8),%rax
   8fcc4:	48 ff c8             	dec    %rax
   8fcc7:	0f 80 7a 23 00 00    	jo     92047 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6bf7>
   8fccd:	48 8b 4c 29 31       	mov    0x31(%rcx,%rbp,1),%rcx
   8fcd2:	48 3b 41 08          	cmp    0x8(%rcx),%rax
   8fcd6:	0f 83 40 2a 00 00    	jae    9271c <_Wfl.fl_vm_exec$a5081fe92876eb37+0x72cc>
   8fcdc:	48 8b 09             	mov    (%rcx),%rcx
   8fcdf:	48 c1 e0 06          	shl    $0x6,%rax
   8fce3:	0f 10 44 01 18       	movups 0x18(%rcx,%rax,1),%xmm0
   8fce8:	48 8b 44 01 28       	mov    0x28(%rcx,%rax,1),%rax
   8fced:	e9 20 14 00 00       	jmp    91112 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5cc2>
   8fcf2:	31 db                	xor    %ebx,%ebx
   8fcf4:	48 81 7c 24 20 00 40 	cmpq   $0x4000,0x20(%rsp)
   8fcfb:	00 00 
   8fcfd:	7c 11                	jl     8fd10 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x48c0>
   8fcff:	4c 89 f7             	mov    %r14,%rdi
   8fd02:	e8 59 78 02 00       	call   b7560 <_Wfl.fl_push_overflow$ec8cb5daf6580c79>
   8fd07:	49 8b 8e ac 00 00 00 	mov    0xac(%r14),%rcx
   8fd0e:	eb 31                	jmp    8fd41 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x48f1>
   8fd10:	48 39 4c 24 20       	cmp    %rcx,0x20(%rsp)
   8fd15:	0f 83 e5 2a 00 00    	jae    92800 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x73b0>
   8fd1b:	48 8b 54 24 20       	mov    0x20(%rsp),%rdx
   8fd20:	48 8d 0c 52          	lea    (%rdx,%rdx,2),%rcx
   8fd24:	48 c7 04 c8 02 00 00 	movq   $0x2,(%rax,%rcx,8)
   8fd2b:	00 
   8fd2c:	48 ff c2             	inc    %rdx
   8fd2f:	0f 57 c0             	xorps  %xmm0,%xmm0
   8fd32:	0f 11 44 c8 08       	movups %xmm0,0x8(%rax,%rcx,8)
   8fd37:	48 89 d1             	mov    %rdx,%rcx
   8fd3a:	49 89 96 ac 00 00 00 	mov    %rdx,0xac(%r14)
   8fd41:	48 81 f9 00 40 00 00 	cmp    $0x4000,%rcx
   8fd48:	0f 8d 01 e8 ff ff    	jge    8e54f <_Wfl.fl_vm_exec$a5081fe92876eb37+0x30ff>
   8fd4e:	49 8b 86 a4 00 00 00 	mov    0xa4(%r14),%rax
   8fd55:	48 3b 48 08          	cmp    0x8(%rax),%rcx
   8fd59:	0f 83 88 2b 00 00    	jae    928e7 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x7497>
   8fd5f:	48 8b 00             	mov    (%rax),%rax
   8fd62:	48 89 ca             	mov    %rcx,%rdx
   8fd65:	48 8d 0c 49          	lea    (%rcx,%rcx,2),%rcx
   8fd69:	48 c7 04 c8 02 00 00 	movq   $0x2,(%rax,%rcx,8)
   8fd70:	00 
   8fd71:	48 89 5c c8 08       	mov    %rbx,0x8(%rax,%rcx,8)
   8fd76:	48 c7 44 c8 10 00 00 	movq   $0x0,0x10(%rax,%rcx,8)
   8fd7d:	00 00 
   8fd7f:	48 ff c2             	inc    %rdx
   8fd82:	49 89 96 ac 00 00 00 	mov    %rdx,0xac(%r14)
   8fd89:	e9 89 f1 ff ff       	jmp    8ef17 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x3ac7>
   8fd8e:	4c 89 bc 24 d0 00 00 	mov    %r15,0xd0(%rsp)
   8fd95:	00 
   8fd96:	48 8d b4 24 d0 00 00 	lea    0xd0(%rsp),%rsi
   8fd9d:	00 
   8fd9e:	e8 ad ec 0b 00       	call   14ea50 <__wolf_rt_list_push>
   8fda3:	4c 89 e7             	mov    %r12,%rdi
   8fda6:	4c 89 fe             	mov    %r15,%rsi
   8fda9:	ff 17                	call   *(%rdi)
   8fdab:	e9 67 f1 ff ff       	jmp    8ef17 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x3ac7>
   8fdb0:	48 8b 3f             	mov    (%rdi),%rdi
   8fdb3:	0f b7 14 17          	movzwl (%rdi,%rdx,1),%edx
   8fdb7:	48 01 d1             	add    %rdx,%rcx
   8fdba:	0f 80 c5 27 00 00    	jo     92585 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x7135>
   8fdc0:	49 3b 71 10          	cmp    0x10(%r9),%rsi
   8fdc4:	73 5d                	jae    8fe23 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x49d3>
   8fdc6:	49 8b 11             	mov    (%r9),%rdx
   8fdc9:	48 89 f7             	mov    %rsi,%rdi
   8fdcc:	48 c1 e7 05          	shl    $0x5,%rdi
   8fdd0:	48 89 0c 3a          	mov    %rcx,(%rdx,%rdi,1)
   8fdd4:	48 8b 4c 24 20       	mov    0x20(%rsp),%rcx
   8fdd9:	48 89 4c 3a 08       	mov    %rcx,0x8(%rdx,%rdi,1)
   8fdde:	48 89 44 3a 10       	mov    %rax,0x10(%rdx,%rdi,1)
   8fde3:	48 89 6c 3a 18       	mov    %rbp,0x18(%rdx,%rdi,1)
   8fde8:	48 ff c6             	inc    %rsi
   8fdeb:	49 89 71 08          	mov    %rsi,0x8(%r9)
   8fdef:	e9 23 f1 ff ff       	jmp    8ef17 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x3ac7>
   8fdf4:	ba 0c 00 00 00       	mov    $0xc,%edx
   8fdf9:	41 b9 37 00 00 00    	mov    $0x37,%r9d
   8fdff:	4c 89 f7             	mov    %r14,%rdi
   8fe02:	48 8d 35 37 4a f8 ff 	lea    -0x7b5c9(%rip),%rsi        # 14840 <__abi_tag+0x14544>
   8fe09:	4c 8b 24 24          	mov    (%rsp),%r12
   8fe0d:	4c 89 e1             	mov    %r12,%rcx
   8fe10:	4c 8b 44 24 70       	mov    0x70(%rsp),%r8
   8fe15:	6a 1e                	push   $0x1e
   8fe17:	48 8d 05 87 4c f8 ff 	lea    -0x7b379(%rip),%rax        # 14aa5 <__abi_tag+0x147a9>
   8fe1e:	e9 95 01 00 00       	jmp    8ffb8 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4b68>
   8fe23:	48 89 8c 24 10 03 00 	mov    %rcx,0x310(%rsp)
   8fe2a:	00 
   8fe2b:	48 8b 4c 24 20       	mov    0x20(%rsp),%rcx
   8fe30:	48 89 8c 24 18 03 00 	mov    %rcx,0x318(%rsp)
   8fe37:	00 
   8fe38:	48 89 84 24 20 03 00 	mov    %rax,0x320(%rsp)
   8fe3f:	00 
   8fe40:	48 89 ac 24 28 03 00 	mov    %rbp,0x328(%rsp)
   8fe47:	00 
   8fe48:	4c 89 cf             	mov    %r9,%rdi
   8fe4b:	48 8d b4 24 10 03 00 	lea    0x310(%rsp),%rsi
   8fe52:	00 
   8fe53:	e8 f8 eb 0b 00       	call   14ea50 <__wolf_rt_list_push>
   8fe58:	e9 ba f0 ff ff       	jmp    8ef17 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x3ac7>
   8fe5d:	41 bf 0f 00 00 00    	mov    $0xf,%r15d
   8fe63:	4c 8d 2d cd 4a f8 ff 	lea    -0x7b533(%rip),%r13        # 14937 <__abi_tag+0x1463b>
   8fe6a:	80 7c 24 28 2c       	cmpb   $0x2c,0x28(%rsp)
   8fe6f:	0f 84 84 f1 ff ff    	je     8eff9 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x3ba9>
   8fe75:	80 7c 24 28 2d       	cmpb   $0x2d,0x28(%rsp)
   8fe7a:	75 7a                	jne    8fef6 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4aa6>
   8fe7c:	48 83 fb 06          	cmp    $0x6,%rbx
   8fe80:	74 74                	je     8fef6 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4aa6>
   8fe82:	e8 e9 90 0c 00       	call   158f70 <__wolf_rt_strbuf_new>
   8fe87:	49 89 c7             	mov    %rax,%r15
   8fe8a:	ba 16 00 00 00       	mov    $0x16,%edx
   8fe8f:	48 89 c7             	mov    %rax,%rdi
   8fe92:	48 8d 35 72 4b f8 ff 	lea    -0x7b48e(%rip),%rsi        # 14a0b <__abi_tag+0x1470f>
   8fe99:	31 c9                	xor    %ecx,%ecx
   8fe9b:	e8 80 91 0c 00       	call   159020 <__wolf_rt_strbuf_str>
   8fea0:	48 89 df             	mov    %rbx,%rdi
   8fea3:	e8 f8 bb 06 00       	call   fbaa0 <_Wfl.fl_type_name$425de879c2cc2a41>
   8fea8:	4c 89 ff             	mov    %r15,%rdi
   8feab:	48 89 c6             	mov    %rax,%rsi
   8feae:	31 c9                	xor    %ecx,%ecx
   8feb0:	e8 6b 91 0c 00       	call   159020 <__wolf_rt_strbuf_str>
   8feb5:	4c 89 ff             	mov    %r15,%rdi
   8feb8:	48 8d b4 24 b0 01 00 	lea    0x1b0(%rsp),%rsi
   8febf:	00 
   8fec0:	e8 5b 8d 0c 00       	call   158c20 <__wolf_rt_strbuf_finish>
   8fec5:	48 8b 8c 24 b0 01 00 	mov    0x1b0(%rsp),%rcx
   8fecc:	00 
   8fecd:	4c 8b 84 24 b8 01 00 	mov    0x1b8(%rsp),%r8
   8fed4:	00 
   8fed5:	ba 04 00 00 00       	mov    $0x4,%edx
   8feda:	4c 89 f7             	mov    %r14,%rdi
   8fedd:	48 8d 35 0c e5 f8 ff 	lea    -0x71af4(%rip),%rsi        # 1e3f0 <__abi_tag+0x1e0f4>
   8fee4:	4c 8d 8c 24 00 1f 00 	lea    0x1f00(%rsp),%r9
   8feeb:	00 
   8feec:	e8 df 2b 08 00       	call   112ad0 <_Wfl.fl_raise_bytes.bytesview.4$e8b5fa06fc8adc2d>
   8fef1:	e9 21 f0 ff ff       	jmp    8ef17 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x3ac7>
   8fef6:	80 7c 24 28 2d       	cmpb   $0x2d,0x28(%rsp)
   8fefb:	0f 85 cb 00 00 00    	jne    8ffcc <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4b7c>
   8ff01:	48 8d 43 fc          	lea    -0x4(%rbx),%rax
   8ff05:	48 83 f8 07          	cmp    $0x7,%rax
   8ff09:	77 47                	ja     8ff52 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4b02>
   8ff0b:	4d 85 e4             	test   %r12,%r12
   8ff0e:	0f 88 cf 24 00 00    	js     923e3 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6f93>
   8ff14:	49 8b 06             	mov    (%r14),%rax
   8ff17:	44 89 e1             	mov    %r12d,%ecx
   8ff1a:	48 3b 48 08          	cmp    0x8(%rax),%rcx
   8ff1e:	0f 8d bf 24 00 00    	jge    923e3 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6f93>
   8ff24:	48 8b 00             	mov    (%rax),%rax
   8ff27:	48 69 c9 60 01 00 00 	imul   $0x160,%rcx,%rcx
   8ff2e:	48 39 1c 08          	cmp    %rbx,(%rax,%rcx,1)
   8ff32:	0f 85 ab 24 00 00    	jne    923e3 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6f93>
   8ff38:	49 c1 ec 20          	shr    $0x20,%r12
   8ff3c:	4c 39 64 08 19       	cmp    %r12,0x19(%rax,%rcx,1)
   8ff41:	0f 85 9c 24 00 00    	jne    923e3 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6f93>
   8ff47:	f6 44 08 09 02       	testb  $0x2,0x9(%rax,%rcx,1)
   8ff4c:	0f 85 bf 06 00 00    	jne    90611 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x51c1>
   8ff52:	4c 89 f7             	mov    %r14,%rdi
   8ff55:	4c 89 ee             	mov    %r13,%rsi
   8ff58:	4c 89 fa             	mov    %r15,%rdx
   8ff5b:	48 8d 9c 24 b8 00 00 	lea    0xb8(%rsp),%rbx
   8ff62:	00 
   8ff63:	48 89 d9             	mov    %rbx,%rcx
   8ff66:	e8 75 6a 05 00       	call   e69e0 <_Wfl.fl_str_text$a33697b96957ba85>
   8ff6b:	4c 89 f7             	mov    %r14,%rdi
   8ff6e:	48 8d b4 24 20 01 00 	lea    0x120(%rsp),%rsi
   8ff75:	00 
   8ff76:	48 89 da             	mov    %rbx,%rdx
   8ff79:	48 8d 8c 24 50 01 00 	lea    0x150(%rsp),%rcx
   8ff80:	00 
   8ff81:	e8 2a f6 02 00       	call   bf5b0 <_Wfl.fl_map_set$540fcc92758450a0>
   8ff86:	84 c0                	test   %al,%al
   8ff88:	0f 85 89 ef ff ff    	jne    8ef17 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x3ac7>
   8ff8e:	ba 0c 00 00 00       	mov    $0xc,%edx
   8ff93:	41 b9 2d 00 00 00    	mov    $0x2d,%r9d
   8ff99:	4c 89 f7             	mov    %r14,%rdi
   8ff9c:	48 8d 35 9d 48 f8 ff 	lea    -0x7b763(%rip),%rsi        # 14840 <__abi_tag+0x14544>
   8ffa3:	4c 8b 24 24          	mov    (%rsp),%r12
   8ffa7:	4c 89 e1             	mov    %r12,%rcx
   8ffaa:	4c 8b 44 24 70       	mov    0x70(%rsp),%r8
   8ffaf:	6a 21                	push   $0x21
   8ffb1:	48 8d 05 69 4a f8 ff 	lea    -0x7b597(%rip),%rax        # 14a21 <__abi_tag+0x14725>
   8ffb8:	50                   	push   %rax
   8ffb9:	e8 a2 cc 06 00       	call   fcc60 <_Wfl.fl_vm_bug_at$494622488d4f7894>
   8ffbe:	48 83 c4 10          	add    $0x10,%rsp
   8ffc2:	48 8b 6c 24 08       	mov    0x8(%rsp),%rbp
   8ffc7:	e9 8f b5 ff ff       	jmp    8b55b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x10b>
   8ffcc:	4c 89 f7             	mov    %r14,%rdi
   8ffcf:	4c 89 ee             	mov    %r13,%rsi
   8ffd2:	4c 89 fa             	mov    %r15,%rdx
   8ffd5:	48 8d 8c 24 b8 00 00 	lea    0xb8(%rsp),%rcx
   8ffdc:	00 
   8ffdd:	e8 fe 69 05 00       	call   e69e0 <_Wfl.fl_str_text$a33697b96957ba85>
   8ffe2:	48 83 fb 06          	cmp    $0x6,%rbx
   8ffe6:	0f 84 ff 07 00 00    	je     907eb <_Wfl.fl_vm_exec$a5081fe92876eb37+0x539b>
   8ffec:	49 8b 4e 20          	mov    0x20(%r14),%rcx
   8fff0:	49 8b 46 48          	mov    0x48(%r14),%rax
   8fff4:	41 0f 10 06          	movups (%r14),%xmm0
   8fff8:	0f 29 84 24 50 14 00 	movaps %xmm0,0x1450(%rsp)
   8ffff:	00 
   90000:	41 0f 10 46 10       	movups 0x10(%r14),%xmm0
   90005:	0f 29 84 24 60 14 00 	movaps %xmm0,0x1460(%rsp)
   9000c:	00 
   9000d:	48 89 8c 24 70 14 00 	mov    %rcx,0x1470(%rsp)
   90014:	00 
   90015:	41 0f b7 4e 50       	movzwl 0x50(%r14),%ecx
   9001a:	41 0f 10 46 28       	movups 0x28(%r14),%xmm0
   9001f:	0f 11 84 24 78 14 00 	movups %xmm0,0x1478(%rsp)
   90026:	00 
   90027:	41 0f 10 46 38       	movups 0x38(%r14),%xmm0
   9002c:	0f 11 84 24 88 14 00 	movups %xmm0,0x1488(%rsp)
   90033:	00 
   90034:	48 89 84 24 98 14 00 	mov    %rax,0x1498(%rsp)
   9003b:	00 
   9003c:	49 8b 46 62          	mov    0x62(%r14),%rax
   90040:	48 89 8c 24 a0 14 00 	mov    %rcx,0x14a0(%rsp)
   90047:	00 
   90048:	49 8b 8e 8a 00 00 00 	mov    0x8a(%r14),%rcx
   9004f:	41 0f 10 46 52       	movups 0x52(%r14),%xmm0
   90054:	0f 11 84 24 a8 14 00 	movups %xmm0,0x14a8(%rsp)
   9005b:	00 
   9005c:	48 89 84 24 b8 14 00 	mov    %rax,0x14b8(%rsp)
   90063:	00 
   90064:	41 0f b7 86 a2 00 00 	movzwl 0xa2(%r14),%eax
   9006b:	00 
   9006c:	41 0f 10 46 6a       	movups 0x6a(%r14),%xmm0
   90071:	0f 29 84 24 c0 14 00 	movaps %xmm0,0x14c0(%rsp)
   90078:	00 
   90079:	41 0f 10 46 7a       	movups 0x7a(%r14),%xmm0
   9007e:	0f 29 84 24 d0 14 00 	movaps %xmm0,0x14d0(%rsp)
   90085:	00 
   90086:	41 0f 10 86 92 00 00 	movups 0x92(%r14),%xmm0
   9008d:	00 
   9008e:	48 89 8c 24 e0 14 00 	mov    %rcx,0x14e0(%rsp)
   90095:	00 
   90096:	0f 11 84 24 e8 14 00 	movups %xmm0,0x14e8(%rsp)
   9009d:	00 
   9009e:	41 0f 10 86 a4 00 00 	movups 0xa4(%r14),%xmm0
   900a5:	00 
   900a6:	49 8b 8e c4 00 00 00 	mov    0xc4(%r14),%rcx
   900ad:	48 89 84 24 f8 14 00 	mov    %rax,0x14f8(%rsp)
   900b4:	00 
   900b5:	41 0f b6 86 5c 01 00 	movzbl 0x15c(%r14),%eax
   900bc:	00 
   900bd:	0f 29 84 24 00 15 00 	movaps %xmm0,0x1500(%rsp)
   900c4:	00 
   900c5:	41 0f 10 86 b4 00 00 	movups 0xb4(%r14),%xmm0
   900cc:	00 
   900cd:	0f 29 84 24 10 15 00 	movaps %xmm0,0x1510(%rsp)
   900d4:	00 
   900d5:	41 0f 10 86 cc 00 00 	movups 0xcc(%r14),%xmm0
   900dc:	00 
   900dd:	48 89 8c 24 20 15 00 	mov    %rcx,0x1520(%rsp)
   900e4:	00 
   900e5:	0f 11 84 24 28 15 00 	movups %xmm0,0x1528(%rsp)
   900ec:	00 
   900ed:	41 0f 10 86 dc 00 00 	movups 0xdc(%r14),%xmm0
   900f4:	00 
   900f5:	0f 11 84 24 38 15 00 	movups %xmm0,0x1538(%rsp)
   900fc:	00 
   900fd:	41 0f 10 86 ec 00 00 	movups 0xec(%r14),%xmm0
   90104:	00 
   90105:	0f 11 84 24 48 15 00 	movups %xmm0,0x1548(%rsp)
   9010c:	00 
   9010d:	41 0f 10 86 fc 00 00 	movups 0xfc(%r14),%xmm0
   90114:	00 
   90115:	0f 11 84 24 58 15 00 	movups %xmm0,0x1558(%rsp)
   9011c:	00 
   9011d:	41 0f 10 86 0c 01 00 	movups 0x10c(%r14),%xmm0
   90124:	00 
   90125:	0f 11 84 24 68 15 00 	movups %xmm0,0x1568(%rsp)
   9012c:	00 
   9012d:	41 0f 10 86 1c 01 00 	movups 0x11c(%r14),%xmm0
   90134:	00 
   90135:	0f 11 84 24 78 15 00 	movups %xmm0,0x1578(%rsp)
   9013c:	00 
   9013d:	41 0f 10 86 2c 01 00 	movups 0x12c(%r14),%xmm0
   90144:	00 
   90145:	0f 11 84 24 88 15 00 	movups %xmm0,0x1588(%rsp)
   9014c:	00 
   9014d:	41 0f 10 86 3c 01 00 	movups 0x13c(%r14),%xmm0
   90154:	00 
   90155:	0f 11 84 24 98 15 00 	movups %xmm0,0x1598(%rsp)
   9015c:	00 
   9015d:	41 0f 10 86 4c 01 00 	movups 0x14c(%r14),%xmm0
   90164:	00 
   90165:	0f 11 84 24 a8 15 00 	movups %xmm0,0x15a8(%rsp)
   9016c:	00 
   9016d:	41 0f 10 86 5d 01 00 	movups 0x15d(%r14),%xmm0
   90174:	00 
   90175:	88 84 24 b8 15 00 00 	mov    %al,0x15b8(%rsp)
   9017c:	0f 29 84 24 c0 15 00 	movaps %xmm0,0x15c0(%rsp)
   90183:	00 
   90184:	41 0f 10 86 6d 01 00 	movups 0x16d(%r14),%xmm0
   9018b:	00 
   9018c:	41 0f b6 86 dd 01 00 	movzbl 0x1dd(%r14),%eax
   90193:	00 
   90194:	49 8b 8e ee 01 00 00 	mov    0x1ee(%r14),%rcx
   9019b:	0f 29 84 24 d0 15 00 	movaps %xmm0,0x15d0(%rsp)
   901a2:	00 
   901a3:	41 0f 10 86 7d 01 00 	movups 0x17d(%r14),%xmm0
   901aa:	00 
   901ab:	0f 29 84 24 e0 15 00 	movaps %xmm0,0x15e0(%rsp)
   901b2:	00 
   901b3:	41 0f 10 86 8d 01 00 	movups 0x18d(%r14),%xmm0
   901ba:	00 
   901bb:	0f 29 84 24 f0 15 00 	movaps %xmm0,0x15f0(%rsp)
   901c2:	00 
   901c3:	41 0f 10 86 9d 01 00 	movups 0x19d(%r14),%xmm0
   901ca:	00 
   901cb:	0f 29 84 24 00 16 00 	movaps %xmm0,0x1600(%rsp)
   901d2:	00 
   901d3:	41 0f 10 86 ad 01 00 	movups 0x1ad(%r14),%xmm0
   901da:	00 
   901db:	0f 29 84 24 10 16 00 	movaps %xmm0,0x1610(%rsp)
   901e2:	00 
   901e3:	41 0f 10 86 bd 01 00 	movups 0x1bd(%r14),%xmm0
   901ea:	00 
   901eb:	0f 29 84 24 20 16 00 	movaps %xmm0,0x1620(%rsp)
   901f2:	00 
   901f3:	41 0f 10 86 cd 01 00 	movups 0x1cd(%r14),%xmm0
   901fa:	00 
   901fb:	0f 29 84 24 30 16 00 	movaps %xmm0,0x1630(%rsp)
   90202:	00 
   90203:	88 84 24 40 16 00 00 	mov    %al,0x1640(%rsp)
   9020a:	49 8b 86 16 02 00 00 	mov    0x216(%r14),%rax
   90211:	41 0f 10 86 de 01 00 	movups 0x1de(%r14),%xmm0
   90218:	00 
   90219:	0f 11 84 24 48 16 00 	movups %xmm0,0x1648(%rsp)
   90220:	00 
   90221:	48 89 8c 24 58 16 00 	mov    %rcx,0x1658(%rsp)
   90228:	00 
   90229:	41 0f b6 8e 1e 02 00 	movzbl 0x21e(%r14),%ecx
   90230:	00 
   90231:	41 0f 10 86 f6 01 00 	movups 0x1f6(%r14),%xmm0
   90238:	00 
   90239:	0f 29 84 24 60 16 00 	movaps %xmm0,0x1660(%rsp)
   90240:	00 
   90241:	41 0f 10 86 06 02 00 	movups 0x206(%r14),%xmm0
   90248:	00 
   90249:	0f 29 84 24 70 16 00 	movaps %xmm0,0x1670(%rsp)
   90250:	00 
   90251:	48 89 84 24 80 16 00 	mov    %rax,0x1680(%rsp)
   90258:	00 
   90259:	49 8b 86 2f 02 00 00 	mov    0x22f(%r14),%rax
   90260:	48 89 8c 24 88 16 00 	mov    %rcx,0x1688(%rsp)
   90267:	00 
   90268:	49 8b 8e 47 02 00 00 	mov    0x247(%r14),%rcx
   9026f:	41 0f 10 86 1f 02 00 	movups 0x21f(%r14),%xmm0
   90276:	00 
   90277:	0f 29 84 24 90 16 00 	movaps %xmm0,0x1690(%rsp)
   9027e:	00 
   9027f:	48 89 84 24 a0 16 00 	mov    %rax,0x16a0(%rsp)
   90286:	00 
   90287:	49 8b 86 7f 02 00 00 	mov    0x27f(%r14),%rax
   9028e:	41 0f 10 86 37 02 00 	movups 0x237(%r14),%xmm0
   90295:	00 
   90296:	0f 11 84 24 a8 16 00 	movups %xmm0,0x16a8(%rsp)
   9029d:	00 
   9029e:	41 0f 10 86 4f 02 00 	movups 0x24f(%r14),%xmm0
   902a5:	00 
   902a6:	48 89 8c 24 b8 16 00 	mov    %rcx,0x16b8(%rsp)
   902ad:	00 
   902ae:	0f 29 84 24 c0 16 00 	movaps %xmm0,0x16c0(%rsp)
   902b5:	00 
   902b6:	41 0f 10 86 5f 02 00 	movups 0x25f(%r14),%xmm0
   902bd:	00 
   902be:	0f 29 84 24 d0 16 00 	movaps %xmm0,0x16d0(%rsp)
   902c5:	00 
   902c6:	41 0f 10 86 6f 02 00 	movups 0x26f(%r14),%xmm0
   902cd:	00 
   902ce:	0f 29 84 24 e0 16 00 	movaps %xmm0,0x16e0(%rsp)
   902d5:	00 
   902d6:	41 0f 10 86 87 02 00 	movups 0x287(%r14),%xmm0
   902dd:	00 
   902de:	48 89 84 24 f0 16 00 	mov    %rax,0x16f0(%rsp)
   902e5:	00 
   902e6:	0f 11 84 24 f8 16 00 	movups %xmm0,0x16f8(%rsp)
   902ed:	00 
   902ee:	41 0f 10 86 97 02 00 	movups 0x297(%r14),%xmm0
   902f5:	00 
   902f6:	0f 11 84 24 08 17 00 	movups %xmm0,0x1708(%rsp)
   902fd:	00 
   902fe:	41 0f 10 86 a7 02 00 	movups 0x2a7(%r14),%xmm0
   90305:	00 
   90306:	0f 11 84 24 18 17 00 	movups %xmm0,0x1718(%rsp)
   9030d:	00 
   9030e:	41 0f 10 86 b7 02 00 	movups 0x2b7(%r14),%xmm0
   90315:	00 
   90316:	0f 11 84 24 28 17 00 	movups %xmm0,0x1728(%rsp)
   9031d:	00 
   9031e:	41 0f 10 86 c7 02 00 	movups 0x2c7(%r14),%xmm0
   90325:	00 
   90326:	0f 11 84 24 38 17 00 	movups %xmm0,0x1738(%rsp)
   9032d:	00 
   9032e:	41 0f 10 86 d7 02 00 	movups 0x2d7(%r14),%xmm0
   90335:	00 
   90336:	0f 11 84 24 48 17 00 	movups %xmm0,0x1748(%rsp)
   9033d:	00 
   9033e:	41 0f 10 86 e7 02 00 	movups 0x2e7(%r14),%xmm0
   90345:	00 
   90346:	0f 11 84 24 58 17 00 	movups %xmm0,0x1758(%rsp)
   9034d:	00 
   9034e:	41 0f 10 86 f7 02 00 	movups 0x2f7(%r14),%xmm0
   90355:	00 
   90356:	0f 11 84 24 68 17 00 	movups %xmm0,0x1768(%rsp)
   9035d:	00 
   9035e:	41 0f 10 86 07 03 00 	movups 0x307(%r14),%xmm0
   90365:	00 
   90366:	49 8b 86 17 03 00 00 	mov    0x317(%r14),%rax
   9036d:	49 8b 8e 3f 03 00 00 	mov    0x33f(%r14),%rcx
   90374:	0f 11 84 24 78 17 00 	movups %xmm0,0x1778(%rsp)
   9037b:	00 
   9037c:	48 89 84 24 88 17 00 	mov    %rax,0x1788(%rsp)
   90383:	00 
   90384:	49 8b 86 57 03 00 00 	mov    0x357(%r14),%rax
   9038b:	41 0f 10 86 1f 03 00 	movups 0x31f(%r14),%xmm0
   90392:	00 
   90393:	0f 29 84 24 90 17 00 	movaps %xmm0,0x1790(%rsp)
   9039a:	00 
   9039b:	41 0f 10 86 2f 03 00 	movups 0x32f(%r14),%xmm0
   903a2:	00 
   903a3:	0f 29 84 24 a0 17 00 	movaps %xmm0,0x17a0(%rsp)
   903aa:	00 
   903ab:	48 89 8c 24 b0 17 00 	mov    %rcx,0x17b0(%rsp)
   903b2:	00 
   903b3:	41 0f b6 8e 5f 03 00 	movzbl 0x35f(%r14),%ecx
   903ba:	00 
   903bb:	41 0f 10 86 47 03 00 	movups 0x347(%r14),%xmm0
   903c2:	00 
   903c3:	0f 11 84 24 b8 17 00 	movups %xmm0,0x17b8(%rsp)
   903ca:	00 
   903cb:	48 89 84 24 c8 17 00 	mov    %rax,0x17c8(%rsp)
   903d2:	00 
   903d3:	49 8b 86 90 03 00 00 	mov    0x390(%r14),%rax
   903da:	48 89 8c 24 d0 17 00 	mov    %rcx,0x17d0(%rsp)
   903e1:	00 
   903e2:	41 0f b6 8e a8 03 00 	movzbl 0x3a8(%r14),%ecx
   903e9:	00 
   903ea:	41 0f 10 86 60 03 00 	movups 0x360(%r14),%xmm0
   903f1:	00 
   903f2:	0f 11 84 24 d8 17 00 	movups %xmm0,0x17d8(%rsp)
   903f9:	00 
   903fa:	41 0f 10 86 70 03 00 	movups 0x370(%r14),%xmm0
   90401:	00 
   90402:	0f 11 84 24 e8 17 00 	movups %xmm0,0x17e8(%rsp)
   90409:	00 
   9040a:	41 0f 10 86 80 03 00 	movups 0x380(%r14),%xmm0
   90411:	00 
   90412:	0f 11 84 24 f8 17 00 	movups %xmm0,0x17f8(%rsp)
   90419:	00 
   9041a:	48 89 84 24 08 18 00 	mov    %rax,0x1808(%rsp)
   90421:	00 
   90422:	49 8b 86 b9 03 00 00 	mov    0x3b9(%r14),%rax
   90429:	41 0f 10 86 98 03 00 	movups 0x398(%r14),%xmm0
   90430:	00 
   90431:	0f 29 84 24 10 18 00 	movaps %xmm0,0x1810(%rsp)
   90438:	00 
   90439:	48 89 8c 24 20 18 00 	mov    %rcx,0x1820(%rsp)
   90440:	00 
   90441:	49 8b 8e 01 04 00 00 	mov    0x401(%r14),%rcx
   90448:	41 0f 10 86 a9 03 00 	movups 0x3a9(%r14),%xmm0
   9044f:	00 
   90450:	0f 11 84 24 28 18 00 	movups %xmm0,0x1828(%rsp)
   90457:	00 
   90458:	41 0f 10 86 c1 03 00 	movups 0x3c1(%r14),%xmm0
   9045f:	00 
   90460:	48 89 84 24 38 18 00 	mov    %rax,0x1838(%rsp)
   90467:	00 
   90468:	0f 29 84 24 40 18 00 	movaps %xmm0,0x1840(%rsp)
   9046f:	00 
   90470:	41 0f 10 86 d1 03 00 	movups 0x3d1(%r14),%xmm0
   90477:	00 
   90478:	0f 29 84 24 50 18 00 	movaps %xmm0,0x1850(%rsp)
   9047f:	00 
   90480:	41 0f 10 86 e1 03 00 	movups 0x3e1(%r14),%xmm0
   90487:	00 
   90488:	0f 29 84 24 60 18 00 	movaps %xmm0,0x1860(%rsp)
   9048f:	00 
   90490:	41 0f 10 86 f1 03 00 	movups 0x3f1(%r14),%xmm0
   90497:	00 
   90498:	0f 29 84 24 70 18 00 	movaps %xmm0,0x1870(%rsp)
   9049f:	00 
   904a0:	41 0f 10 86 09 04 00 	movups 0x409(%r14),%xmm0
   904a7:	00 
   904a8:	48 89 8c 24 80 18 00 	mov    %rcx,0x1880(%rsp)
   904af:	00 
   904b0:	0f 11 84 24 88 18 00 	movups %xmm0,0x1888(%rsp)
   904b7:	00 
   904b8:	41 0f 10 86 19 04 00 	movups 0x419(%r14),%xmm0
   904bf:	00 
   904c0:	41 0f b6 86 69 04 00 	movzbl 0x469(%r14),%eax
   904c7:	00 
   904c8:	41 0f b7 8e 6a 04 00 	movzwl 0x46a(%r14),%ecx
   904cf:	00 
   904d0:	c1 e1 08             	shl    $0x8,%ecx
   904d3:	48 09 c1             	or     %rax,%rcx
   904d6:	49 8b 86 49 04 00 00 	mov    0x449(%r14),%rax
   904dd:	0f 11 84 24 98 18 00 	movups %xmm0,0x1898(%rsp)
   904e4:	00 
   904e5:	41 0f 10 86 29 04 00 	movups 0x429(%r14),%xmm0
   904ec:	00 
   904ed:	0f 11 84 24 a8 18 00 	movups %xmm0,0x18a8(%rsp)
   904f4:	00 
   904f5:	41 0f 10 86 39 04 00 	movups 0x439(%r14),%xmm0
   904fc:	00 
   904fd:	0f 11 84 24 b8 18 00 	movups %xmm0,0x18b8(%rsp)
   90504:	00 
   90505:	48 89 84 24 c8 18 00 	mov    %rax,0x18c8(%rsp)
   9050c:	00 
   9050d:	49 8b 86 61 04 00 00 	mov    0x461(%r14),%rax
   90514:	41 0f 10 86 51 04 00 	movups 0x451(%r14),%xmm0
   9051b:	00 
   9051c:	0f 29 84 24 d0 18 00 	movaps %xmm0,0x18d0(%rsp)
   90523:	00 
   90524:	48 89 84 24 e0 18 00 	mov    %rax,0x18e0(%rsp)
   9052b:	00 
   9052c:	41 0f 10 86 6c 04 00 	movups 0x46c(%r14),%xmm0
   90533:	00 
   90534:	48 89 8c 24 e8 18 00 	mov    %rcx,0x18e8(%rsp)
   9053b:	00 
   9053c:	0f 29 84 24 f0 18 00 	movaps %xmm0,0x18f0(%rsp)
   90543:	00 
   90544:	41 0f 10 86 7c 04 00 	movups 0x47c(%r14),%xmm0
   9054b:	00 
   9054c:	41 0f b7 86 8c 04 00 	movzwl 0x48c(%r14),%eax
   90553:	00 
   90554:	0f 29 84 24 00 19 00 	movaps %xmm0,0x1900(%rsp)
   9055b:	00 
   9055c:	41 0f 10 86 8e 04 00 	movups 0x48e(%r14),%xmm0
   90563:	00 
   90564:	48 89 84 24 10 19 00 	mov    %rax,0x1910(%rsp)
   9056b:	00 
   9056c:	0f 11 84 24 18 19 00 	movups %xmm0,0x1918(%rsp)
   90573:	00 
   90574:	41 0f 10 86 9e 04 00 	movups 0x49e(%r14),%xmm0
   9057b:	00 
   9057c:	41 0f b6 86 ae 04 00 	movzbl 0x4ae(%r14),%eax
   90583:	00 
   90584:	0f 11 84 24 28 19 00 	movups %xmm0,0x1928(%rsp)
   9058b:	00 
   9058c:	41 0f 10 86 af 04 00 	movups 0x4af(%r14),%xmm0
   90593:	00 
   90594:	88 84 24 38 19 00 00 	mov    %al,0x1938(%rsp)
   9059b:	0f 29 84 24 40 19 00 	movaps %xmm0,0x1940(%rsp)
   905a2:	00 
   905a3:	41 0f 10 86 bf 04 00 	movups 0x4bf(%r14),%xmm0
   905aa:	00 
   905ab:	0f 29 84 24 50 19 00 	movaps %xmm0,0x1950(%rsp)
   905b2:	00 
   905b3:	41 0f 10 86 cf 04 00 	movups 0x4cf(%r14),%xmm0
   905ba:	00 
   905bb:	0f 29 84 24 60 19 00 	movaps %xmm0,0x1960(%rsp)
   905c2:	00 
   905c3:	49 8b 86 df 04 00 00 	mov    0x4df(%r14),%rax
   905ca:	48 89 84 24 70 19 00 	mov    %rax,0x1970(%rsp)
   905d1:	00 
   905d2:	48 8d bc 24 50 14 00 	lea    0x1450(%rsp),%rdi
   905d9:	00 
   905da:	48 8d b4 24 20 01 00 	lea    0x120(%rsp),%rsi
   905e1:	00 
   905e2:	4c 89 ea             	mov    %r13,%rdx
   905e5:	4c 89 f9             	mov    %r15,%rcx
   905e8:	e8 03 c3 06 00       	call   fc8f0 <_Wfl.fl_missing_field$f6f60a2fd600a704>
   905ed:	ba 03 00 00 00       	mov    $0x3,%edx
   905f2:	4c 89 f7             	mov    %r14,%rdi
   905f5:	48 8d 35 a3 41 f8 ff 	lea    -0x7be5d(%rip),%rsi        # 1479f <__abi_tag+0x144a3>
   905fc:	48 89 c1             	mov    %rax,%rcx
   905ff:	4c 8d 84 24 d0 1e 00 	lea    0x1ed0(%rsp),%r8
   90606:	00 
   90607:	e8 e4 6d 02 00       	call   b73f0 <_Wfl.fl_raise_bytes$05a094d3779625bf>
   9060c:	e9 06 e9 ff ff       	jmp    8ef17 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x3ac7>
   90611:	ba 04 00 00 00       	mov    $0x4,%edx
   90616:	41 b8 10 00 00 00    	mov    $0x10,%r8d
   9061c:	4c 89 f7             	mov    %r14,%rdi
   9061f:	48 8d 35 ca dd f8 ff 	lea    -0x72236(%rip),%rsi        # 1e3f0 <__abi_tag+0x1e0f4>
   90626:	48 8d 0d 43 d2 f8 ff 	lea    -0x72dbd(%rip),%rcx        # 1d870 <__abi_tag+0x1d574>
   9062d:	4c 8d 8c 24 e8 1e 00 	lea    0x1ee8(%rsp),%r9
   90634:	00 
   90635:	e8 96 24 08 00       	call   112ad0 <_Wfl.fl_raise_bytes.bytesview.4$e8b5fa06fc8adc2d>
   9063a:	e9 d8 e8 ff ff       	jmp    8ef17 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x3ac7>
   9063f:	49 83 ff 04          	cmp    $0x4,%r15
   90643:	0f 87 42 09 00 00    	ja     90f8b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5b3b>
   90649:	b8 16 00 00 00       	mov    $0x16,%eax
   9064e:	4c 0f a3 f8          	bt     %r15,%rax
   90652:	0f 83 33 09 00 00    	jae    90f8b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5b3b>
   90658:	48 85 f6             	test   %rsi,%rsi
   9065b:	0f 88 9a 20 00 00    	js     926fb <_Wfl.fl_vm_exec$a5081fe92876eb37+0x72ab>
   90661:	89 f3                	mov    %esi,%ebx
   90663:	49 3b 5d 08          	cmp    0x8(%r13),%rbx
   90667:	0f 8d 8e 20 00 00    	jge    926fb <_Wfl.fl_vm_exec$a5081fe92876eb37+0x72ab>
   9066d:	49 8b 45 00          	mov    0x0(%r13),%rax
   90671:	4c 69 e3 60 01 00 00 	imul   $0x160,%rbx,%r12
   90678:	4a 83 3c 20 06       	cmpq   $0x6,(%rax,%r12,1)
   9067d:	0f 85 78 20 00 00    	jne    926fb <_Wfl.fl_vm_exec$a5081fe92876eb37+0x72ab>
   90683:	48 c1 ee 20          	shr    $0x20,%rsi
   90687:	4a 39 74 20 19       	cmp    %rsi,0x19(%rax,%r12,1)
   9068c:	0f 85 69 20 00 00    	jne    926fb <_Wfl.fl_vm_exec$a5081fe92876eb37+0x72ab>
   90692:	49 83 ff 01          	cmp    $0x1,%r15
   90696:	0f 84 f3 07 00 00    	je     90e8f <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5a3f>
   9069c:	49 83 ff 04          	cmp    $0x4,%r15
   906a0:	0f 85 fe 07 00 00    	jne    90ea4 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5a54>
   906a6:	48 8d bc 24 f0 09 00 	lea    0x9f0(%rsp),%rdi
   906ad:	00 
   906ae:	48 8d b4 24 38 01 00 	lea    0x138(%rsp),%rsi
   906b5:	00 
   906b6:	e8 35 65 03 00       	call   c6bf0 <_Wfl.fl_obj_index$6801bb8d311b3e64>
   906bb:	49 3b 45 08          	cmp    0x8(%r13),%rax
   906bf:	0f 83 a7 1f 00 00    	jae    9266c <_Wfl.fl_vm_exec$a5081fe92876eb37+0x721c>
   906c5:	49 8b 4d 00          	mov    0x0(%r13),%rcx
   906c9:	48 69 c0 60 01 00 00 	imul   $0x160,%rax,%rax
   906d0:	48 8b 4c 01 11       	mov    0x11(%rcx,%rax,1),%rcx
   906d5:	e9 45 08 00 00       	jmp    90f1f <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5acf>
   906da:	b0 01                	mov    $0x1,%al
   906dc:	84 c0                	test   %al,%al
   906de:	0f 85 60 e9 ff ff    	jne    8f044 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x3bf4>
   906e4:	49 8b 86 ac 00 00 00 	mov    0xac(%r14),%rax
   906eb:	48 3d 00 40 00 00    	cmp    $0x4000,%rax
   906f1:	0f 8d d7 ad ff ff    	jge    8b4ce <_Wfl.fl_vm_exec$a5081fe92876eb37+0x7e>
   906f7:	49 8b 8e a4 00 00 00 	mov    0xa4(%r14),%rcx
   906fe:	48 3b 41 08          	cmp    0x8(%rcx),%rax
   90702:	0f 83 6e 22 00 00    	jae    92976 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x7526>
   90708:	0f 10 84 24 38 03 00 	movups 0x338(%rsp),%xmm0
   9070f:	00 
   90710:	f2 0f 10 8c 24 48 03 	movsd  0x348(%rsp),%xmm1
   90717:	00 00 
   90719:	48 8b 09             	mov    (%rcx),%rcx
   9071c:	48 8d 14 40          	lea    (%rax,%rax,2),%rdx
   90720:	0f 11 04 d1          	movups %xmm0,(%rcx,%rdx,8)
   90724:	f2 0f 11 4c d1 10    	movsd  %xmm1,0x10(%rcx,%rdx,8)
   9072a:	48 ff c0             	inc    %rax
   9072d:	49 89 86 ac 00 00 00 	mov    %rax,0xac(%r14)
   90734:	e9 9d ad ff ff       	jmp    8b4d6 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x86>
   90739:	49 83 ff 04          	cmp    $0x4,%r15
   9073d:	0f 87 91 09 00 00    	ja     910d4 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5c84>
   90743:	b8 16 00 00 00       	mov    $0x16,%eax
   90748:	4c 0f a3 f8          	bt     %r15,%rax
   9074c:	0f 83 82 09 00 00    	jae    910d4 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5c84>
   90752:	49 8d 44 24 f4       	lea    -0xc(%r12),%rax
   90757:	48 83 f8 f8          	cmp    $0xfffffffffffffff8,%rax
   9075b:	0f 82 10 20 00 00    	jb     92771 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x7321>
   90761:	4d 85 ed             	test   %r13,%r13
   90764:	0f 88 07 20 00 00    	js     92771 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x7321>
   9076a:	44 89 eb             	mov    %r13d,%ebx
   9076d:	48 8b 44 24 10       	mov    0x10(%rsp),%rax
   90772:	48 3b 58 08          	cmp    0x8(%rax),%rbx
   90776:	0f 8d f5 1f 00 00    	jge    92771 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x7321>
   9077c:	48 8b 44 24 10       	mov    0x10(%rsp),%rax
   90781:	48 8b 00             	mov    (%rax),%rax
   90784:	48 69 eb 60 01 00 00 	imul   $0x160,%rbx,%rbp
   9078b:	4c 39 24 28          	cmp    %r12,(%rax,%rbp,1)
   9078f:	0f 85 dc 1f 00 00    	jne    92771 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x7321>
   90795:	49 c1 ed 20          	shr    $0x20,%r13
   90799:	4c 39 6c 28 19       	cmp    %r13,0x19(%rax,%rbp,1)
   9079e:	0f 85 cd 1f 00 00    	jne    92771 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x7321>
   907a4:	49 83 ff 04          	cmp    $0x4,%r15
   907a8:	0f 85 65 08 00 00    	jne    91013 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5bc3>
   907ae:	48 8d bc 24 c0 04 00 	lea    0x4c0(%rsp),%rdi
   907b5:	00 
   907b6:	48 8d b4 24 90 00 00 	lea    0x90(%rsp),%rsi
   907bd:	00 
   907be:	e8 2d 64 03 00       	call   c6bf0 <_Wfl.fl_obj_index$6801bb8d311b3e64>
   907c3:	48 8b 4c 24 10       	mov    0x10(%rsp),%rcx
   907c8:	48 3b 41 08          	cmp    0x8(%rcx),%rax
   907cc:	0f 83 4a 1f 00 00    	jae    9271c <_Wfl.fl_vm_exec$a5081fe92876eb37+0x72cc>
   907d2:	48 8b 4c 24 10       	mov    0x10(%rsp),%rcx
   907d7:	48 8b 09             	mov    (%rcx),%rcx
   907da:	48 69 c0 60 01 00 00 	imul   $0x160,%rax,%rax
   907e1:	48 8b 4c 01 11       	mov    0x11(%rcx,%rax,1),%rcx
   907e6:	e9 c9 08 00 00       	jmp    910b4 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5c64>
   907eb:	49 8b 4e 20          	mov    0x20(%r14),%rcx
   907ef:	49 8b 46 48          	mov    0x48(%r14),%rax
   907f3:	41 0f 10 06          	movups (%r14),%xmm0
   907f7:	0f 29 84 24 20 0f 00 	movaps %xmm0,0xf20(%rsp)
   907fe:	00 
   907ff:	41 0f 10 46 10       	movups 0x10(%r14),%xmm0
   90804:	0f 29 84 24 30 0f 00 	movaps %xmm0,0xf30(%rsp)
   9080b:	00 
   9080c:	48 89 8c 24 40 0f 00 	mov    %rcx,0xf40(%rsp)
   90813:	00 
   90814:	41 0f b7 4e 50       	movzwl 0x50(%r14),%ecx
   90819:	41 0f 10 46 28       	movups 0x28(%r14),%xmm0
   9081e:	0f 11 84 24 48 0f 00 	movups %xmm0,0xf48(%rsp)
   90825:	00 
   90826:	41 0f 10 46 38       	movups 0x38(%r14),%xmm0
   9082b:	0f 11 84 24 58 0f 00 	movups %xmm0,0xf58(%rsp)
   90832:	00 
   90833:	48 89 84 24 68 0f 00 	mov    %rax,0xf68(%rsp)
   9083a:	00 
   9083b:	49 8b 46 62          	mov    0x62(%r14),%rax
   9083f:	48 89 8c 24 70 0f 00 	mov    %rcx,0xf70(%rsp)
   90846:	00 
   90847:	49 8b 8e 8a 00 00 00 	mov    0x8a(%r14),%rcx
   9084e:	41 0f 10 46 52       	movups 0x52(%r14),%xmm0
   90853:	0f 11 84 24 78 0f 00 	movups %xmm0,0xf78(%rsp)
   9085a:	00 
   9085b:	48 89 84 24 88 0f 00 	mov    %rax,0xf88(%rsp)
   90862:	00 
   90863:	41 0f b7 86 a2 00 00 	movzwl 0xa2(%r14),%eax
   9086a:	00 
   9086b:	41 0f 10 46 6a       	movups 0x6a(%r14),%xmm0
   90870:	0f 29 84 24 90 0f 00 	movaps %xmm0,0xf90(%rsp)
   90877:	00 
   90878:	41 0f 10 46 7a       	movups 0x7a(%r14),%xmm0
   9087d:	0f 29 84 24 a0 0f 00 	movaps %xmm0,0xfa0(%rsp)
   90884:	00 
   90885:	41 0f 10 86 92 00 00 	movups 0x92(%r14),%xmm0
   9088c:	00 
   9088d:	48 89 8c 24 b0 0f 00 	mov    %rcx,0xfb0(%rsp)
   90894:	00 
   90895:	0f 11 84 24 b8 0f 00 	movups %xmm0,0xfb8(%rsp)
   9089c:	00 
   9089d:	41 0f 10 86 a4 00 00 	movups 0xa4(%r14),%xmm0
   908a4:	00 
   908a5:	49 8b 8e c4 00 00 00 	mov    0xc4(%r14),%rcx
   908ac:	48 89 84 24 c8 0f 00 	mov    %rax,0xfc8(%rsp)
   908b3:	00 
   908b4:	41 0f b6 86 5c 01 00 	movzbl 0x15c(%r14),%eax
   908bb:	00 
   908bc:	0f 29 84 24 d0 0f 00 	movaps %xmm0,0xfd0(%rsp)
   908c3:	00 
   908c4:	41 0f 10 86 b4 00 00 	movups 0xb4(%r14),%xmm0
   908cb:	00 
   908cc:	0f 29 84 24 e0 0f 00 	movaps %xmm0,0xfe0(%rsp)
   908d3:	00 
   908d4:	41 0f 10 86 cc 00 00 	movups 0xcc(%r14),%xmm0
   908db:	00 
   908dc:	48 89 8c 24 f0 0f 00 	mov    %rcx,0xff0(%rsp)
   908e3:	00 
   908e4:	0f 11 84 24 f8 0f 00 	movups %xmm0,0xff8(%rsp)
   908eb:	00 
   908ec:	41 0f 10 86 dc 00 00 	movups 0xdc(%r14),%xmm0
   908f3:	00 
   908f4:	0f 11 84 24 08 10 00 	movups %xmm0,0x1008(%rsp)
   908fb:	00 
   908fc:	41 0f 10 86 ec 00 00 	movups 0xec(%r14),%xmm0
   90903:	00 
   90904:	0f 11 84 24 18 10 00 	movups %xmm0,0x1018(%rsp)
   9090b:	00 
   9090c:	41 0f 10 86 fc 00 00 	movups 0xfc(%r14),%xmm0
   90913:	00 
   90914:	0f 11 84 24 28 10 00 	movups %xmm0,0x1028(%rsp)
   9091b:	00 
   9091c:	41 0f 10 86 0c 01 00 	movups 0x10c(%r14),%xmm0
   90923:	00 
   90924:	0f 11 84 24 38 10 00 	movups %xmm0,0x1038(%rsp)
   9092b:	00 
   9092c:	41 0f 10 86 1c 01 00 	movups 0x11c(%r14),%xmm0
   90933:	00 
   90934:	0f 11 84 24 48 10 00 	movups %xmm0,0x1048(%rsp)
   9093b:	00 
   9093c:	41 0f 10 86 2c 01 00 	movups 0x12c(%r14),%xmm0
   90943:	00 
   90944:	0f 11 84 24 58 10 00 	movups %xmm0,0x1058(%rsp)
   9094b:	00 
   9094c:	41 0f 10 86 3c 01 00 	movups 0x13c(%r14),%xmm0
   90953:	00 
   90954:	0f 11 84 24 68 10 00 	movups %xmm0,0x1068(%rsp)
   9095b:	00 
   9095c:	41 0f 10 86 4c 01 00 	movups 0x14c(%r14),%xmm0
   90963:	00 
   90964:	0f 11 84 24 78 10 00 	movups %xmm0,0x1078(%rsp)
   9096b:	00 
   9096c:	41 0f 10 86 5d 01 00 	movups 0x15d(%r14),%xmm0
   90973:	00 
   90974:	88 84 24 88 10 00 00 	mov    %al,0x1088(%rsp)
   9097b:	0f 29 84 24 90 10 00 	movaps %xmm0,0x1090(%rsp)
   90982:	00 
   90983:	41 0f 10 86 6d 01 00 	movups 0x16d(%r14),%xmm0
   9098a:	00 
   9098b:	41 0f b6 86 dd 01 00 	movzbl 0x1dd(%r14),%eax
   90992:	00 
   90993:	49 8b 8e ee 01 00 00 	mov    0x1ee(%r14),%rcx
   9099a:	0f 29 84 24 a0 10 00 	movaps %xmm0,0x10a0(%rsp)
   909a1:	00 
   909a2:	41 0f 10 86 7d 01 00 	movups 0x17d(%r14),%xmm0
   909a9:	00 
   909aa:	0f 29 84 24 b0 10 00 	movaps %xmm0,0x10b0(%rsp)
   909b1:	00 
   909b2:	41 0f 10 86 8d 01 00 	movups 0x18d(%r14),%xmm0
   909b9:	00 
   909ba:	0f 29 84 24 c0 10 00 	movaps %xmm0,0x10c0(%rsp)
   909c1:	00 
   909c2:	41 0f 10 86 9d 01 00 	movups 0x19d(%r14),%xmm0
   909c9:	00 
   909ca:	0f 29 84 24 d0 10 00 	movaps %xmm0,0x10d0(%rsp)
   909d1:	00 
   909d2:	41 0f 10 86 ad 01 00 	movups 0x1ad(%r14),%xmm0
   909d9:	00 
   909da:	0f 29 84 24 e0 10 00 	movaps %xmm0,0x10e0(%rsp)
   909e1:	00 
   909e2:	41 0f 10 86 bd 01 00 	movups 0x1bd(%r14),%xmm0
   909e9:	00 
   909ea:	0f 29 84 24 f0 10 00 	movaps %xmm0,0x10f0(%rsp)
   909f1:	00 
   909f2:	41 0f 10 86 cd 01 00 	movups 0x1cd(%r14),%xmm0
   909f9:	00 
   909fa:	0f 29 84 24 00 11 00 	movaps %xmm0,0x1100(%rsp)
   90a01:	00 
   90a02:	88 84 24 10 11 00 00 	mov    %al,0x1110(%rsp)
   90a09:	49 8b 86 16 02 00 00 	mov    0x216(%r14),%rax
   90a10:	41 0f 10 86 de 01 00 	movups 0x1de(%r14),%xmm0
   90a17:	00 
   90a18:	0f 11 84 24 18 11 00 	movups %xmm0,0x1118(%rsp)
   90a1f:	00 
   90a20:	48 89 8c 24 28 11 00 	mov    %rcx,0x1128(%rsp)
   90a27:	00 
   90a28:	41 0f b6 8e 1e 02 00 	movzbl 0x21e(%r14),%ecx
   90a2f:	00 
   90a30:	41 0f 10 86 f6 01 00 	movups 0x1f6(%r14),%xmm0
   90a37:	00 
   90a38:	0f 29 84 24 30 11 00 	movaps %xmm0,0x1130(%rsp)
   90a3f:	00 
   90a40:	41 0f 10 86 06 02 00 	movups 0x206(%r14),%xmm0
   90a47:	00 
   90a48:	0f 29 84 24 40 11 00 	movaps %xmm0,0x1140(%rsp)
   90a4f:	00 
   90a50:	48 89 84 24 50 11 00 	mov    %rax,0x1150(%rsp)
   90a57:	00 
   90a58:	49 8b 86 2f 02 00 00 	mov    0x22f(%r14),%rax
   90a5f:	48 89 8c 24 58 11 00 	mov    %rcx,0x1158(%rsp)
   90a66:	00 
   90a67:	49 8b 8e 47 02 00 00 	mov    0x247(%r14),%rcx
   90a6e:	41 0f 10 86 1f 02 00 	movups 0x21f(%r14),%xmm0
   90a75:	00 
   90a76:	0f 29 84 24 60 11 00 	movaps %xmm0,0x1160(%rsp)
   90a7d:	00 
   90a7e:	48 89 84 24 70 11 00 	mov    %rax,0x1170(%rsp)
   90a85:	00 
   90a86:	49 8b 86 7f 02 00 00 	mov    0x27f(%r14),%rax
   90a8d:	41 0f 10 86 37 02 00 	movups 0x237(%r14),%xmm0
   90a94:	00 
   90a95:	0f 11 84 24 78 11 00 	movups %xmm0,0x1178(%rsp)
   90a9c:	00 
   90a9d:	41 0f 10 86 4f 02 00 	movups 0x24f(%r14),%xmm0
   90aa4:	00 
   90aa5:	48 89 8c 24 88 11 00 	mov    %rcx,0x1188(%rsp)
   90aac:	00 
   90aad:	0f 29 84 24 90 11 00 	movaps %xmm0,0x1190(%rsp)
   90ab4:	00 
   90ab5:	41 0f 10 86 5f 02 00 	movups 0x25f(%r14),%xmm0
   90abc:	00 
   90abd:	0f 29 84 24 a0 11 00 	movaps %xmm0,0x11a0(%rsp)
   90ac4:	00 
   90ac5:	41 0f 10 86 6f 02 00 	movups 0x26f(%r14),%xmm0
   90acc:	00 
   90acd:	0f 29 84 24 b0 11 00 	movaps %xmm0,0x11b0(%rsp)
   90ad4:	00 
   90ad5:	41 0f 10 86 87 02 00 	movups 0x287(%r14),%xmm0
   90adc:	00 
   90add:	48 89 84 24 c0 11 00 	mov    %rax,0x11c0(%rsp)
   90ae4:	00 
   90ae5:	0f 11 84 24 c8 11 00 	movups %xmm0,0x11c8(%rsp)
   90aec:	00 
   90aed:	41 0f 10 86 97 02 00 	movups 0x297(%r14),%xmm0
   90af4:	00 
   90af5:	0f 11 84 24 d8 11 00 	movups %xmm0,0x11d8(%rsp)
   90afc:	00 
   90afd:	41 0f 10 86 a7 02 00 	movups 0x2a7(%r14),%xmm0
   90b04:	00 
   90b05:	0f 11 84 24 e8 11 00 	movups %xmm0,0x11e8(%rsp)
   90b0c:	00 
   90b0d:	41 0f 10 86 b7 02 00 	movups 0x2b7(%r14),%xmm0
   90b14:	00 
   90b15:	0f 11 84 24 f8 11 00 	movups %xmm0,0x11f8(%rsp)
   90b1c:	00 
   90b1d:	41 0f 10 86 c7 02 00 	movups 0x2c7(%r14),%xmm0
   90b24:	00 
   90b25:	0f 11 84 24 08 12 00 	movups %xmm0,0x1208(%rsp)
   90b2c:	00 
   90b2d:	41 0f 10 86 d7 02 00 	movups 0x2d7(%r14),%xmm0
   90b34:	00 
   90b35:	0f 11 84 24 18 12 00 	movups %xmm0,0x1218(%rsp)
   90b3c:	00 
   90b3d:	41 0f 10 86 e7 02 00 	movups 0x2e7(%r14),%xmm0
   90b44:	00 
   90b45:	0f 11 84 24 28 12 00 	movups %xmm0,0x1228(%rsp)
   90b4c:	00 
   90b4d:	41 0f 10 86 f7 02 00 	movups 0x2f7(%r14),%xmm0
   90b54:	00 
   90b55:	0f 11 84 24 38 12 00 	movups %xmm0,0x1238(%rsp)
   90b5c:	00 
   90b5d:	41 0f 10 86 07 03 00 	movups 0x307(%r14),%xmm0
   90b64:	00 
   90b65:	49 8b 86 17 03 00 00 	mov    0x317(%r14),%rax
   90b6c:	49 8b 8e 3f 03 00 00 	mov    0x33f(%r14),%rcx
   90b73:	0f 11 84 24 48 12 00 	movups %xmm0,0x1248(%rsp)
   90b7a:	00 
   90b7b:	48 89 84 24 58 12 00 	mov    %rax,0x1258(%rsp)
   90b82:	00 
   90b83:	49 8b 86 57 03 00 00 	mov    0x357(%r14),%rax
   90b8a:	41 0f 10 86 1f 03 00 	movups 0x31f(%r14),%xmm0
   90b91:	00 
   90b92:	0f 29 84 24 60 12 00 	movaps %xmm0,0x1260(%rsp)
   90b99:	00 
   90b9a:	41 0f 10 86 2f 03 00 	movups 0x32f(%r14),%xmm0
   90ba1:	00 
   90ba2:	0f 29 84 24 70 12 00 	movaps %xmm0,0x1270(%rsp)
   90ba9:	00 
   90baa:	48 89 8c 24 80 12 00 	mov    %rcx,0x1280(%rsp)
   90bb1:	00 
   90bb2:	41 0f b6 8e 5f 03 00 	movzbl 0x35f(%r14),%ecx
   90bb9:	00 
   90bba:	41 0f 10 86 47 03 00 	movups 0x347(%r14),%xmm0
   90bc1:	00 
   90bc2:	0f 11 84 24 88 12 00 	movups %xmm0,0x1288(%rsp)
   90bc9:	00 
   90bca:	48 89 84 24 98 12 00 	mov    %rax,0x1298(%rsp)
   90bd1:	00 
   90bd2:	49 8b 86 90 03 00 00 	mov    0x390(%r14),%rax
   90bd9:	48 89 8c 24 a0 12 00 	mov    %rcx,0x12a0(%rsp)
   90be0:	00 
   90be1:	41 0f b6 8e a8 03 00 	movzbl 0x3a8(%r14),%ecx
   90be8:	00 
   90be9:	41 0f 10 86 60 03 00 	movups 0x360(%r14),%xmm0
   90bf0:	00 
   90bf1:	0f 11 84 24 a8 12 00 	movups %xmm0,0x12a8(%rsp)
   90bf8:	00 
   90bf9:	41 0f 10 86 70 03 00 	movups 0x370(%r14),%xmm0
   90c00:	00 
   90c01:	0f 11 84 24 b8 12 00 	movups %xmm0,0x12b8(%rsp)
   90c08:	00 
   90c09:	41 0f 10 86 80 03 00 	movups 0x380(%r14),%xmm0
   90c10:	00 
   90c11:	0f 11 84 24 c8 12 00 	movups %xmm0,0x12c8(%rsp)
   90c18:	00 
   90c19:	48 89 84 24 d8 12 00 	mov    %rax,0x12d8(%rsp)
   90c20:	00 
   90c21:	49 8b 86 b9 03 00 00 	mov    0x3b9(%r14),%rax
   90c28:	41 0f 10 86 98 03 00 	movups 0x398(%r14),%xmm0
   90c2f:	00 
   90c30:	0f 29 84 24 e0 12 00 	movaps %xmm0,0x12e0(%rsp)
   90c37:	00 
   90c38:	48 89 8c 24 f0 12 00 	mov    %rcx,0x12f0(%rsp)
   90c3f:	00 
   90c40:	49 8b 8e 01 04 00 00 	mov    0x401(%r14),%rcx
   90c47:	41 0f 10 86 a9 03 00 	movups 0x3a9(%r14),%xmm0
   90c4e:	00 
   90c4f:	0f 11 84 24 f8 12 00 	movups %xmm0,0x12f8(%rsp)
   90c56:	00 
   90c57:	41 0f 10 86 c1 03 00 	movups 0x3c1(%r14),%xmm0
   90c5e:	00 
   90c5f:	48 89 84 24 08 13 00 	mov    %rax,0x1308(%rsp)
   90c66:	00 
   90c67:	0f 29 84 24 10 13 00 	movaps %xmm0,0x1310(%rsp)
   90c6e:	00 
   90c6f:	41 0f 10 86 d1 03 00 	movups 0x3d1(%r14),%xmm0
   90c76:	00 
   90c77:	0f 29 84 24 20 13 00 	movaps %xmm0,0x1320(%rsp)
   90c7e:	00 
   90c7f:	41 0f 10 86 e1 03 00 	movups 0x3e1(%r14),%xmm0
   90c86:	00 
   90c87:	0f 29 84 24 30 13 00 	movaps %xmm0,0x1330(%rsp)
   90c8e:	00 
   90c8f:	41 0f 10 86 f1 03 00 	movups 0x3f1(%r14),%xmm0
   90c96:	00 
   90c97:	0f 29 84 24 40 13 00 	movaps %xmm0,0x1340(%rsp)
   90c9e:	00 
   90c9f:	41 0f 10 86 09 04 00 	movups 0x409(%r14),%xmm0
   90ca6:	00 
   90ca7:	48 89 8c 24 50 13 00 	mov    %rcx,0x1350(%rsp)
   90cae:	00 
   90caf:	0f 11 84 24 58 13 00 	movups %xmm0,0x1358(%rsp)
   90cb6:	00 
   90cb7:	41 0f 10 86 19 04 00 	movups 0x419(%r14),%xmm0
   90cbe:	00 
   90cbf:	41 0f b6 86 69 04 00 	movzbl 0x469(%r14),%eax
   90cc6:	00 
   90cc7:	41 0f b7 8e 6a 04 00 	movzwl 0x46a(%r14),%ecx
   90cce:	00 
   90ccf:	c1 e1 08             	shl    $0x8,%ecx
   90cd2:	48 09 c1             	or     %rax,%rcx
   90cd5:	49 8b 86 49 04 00 00 	mov    0x449(%r14),%rax
   90cdc:	0f 11 84 24 68 13 00 	movups %xmm0,0x1368(%rsp)
   90ce3:	00 
   90ce4:	41 0f 10 86 29 04 00 	movups 0x429(%r14),%xmm0
   90ceb:	00 
   90cec:	0f 11 84 24 78 13 00 	movups %xmm0,0x1378(%rsp)
   90cf3:	00 
   90cf4:	41 0f 10 86 39 04 00 	movups 0x439(%r14),%xmm0
   90cfb:	00 
   90cfc:	0f 11 84 24 88 13 00 	movups %xmm0,0x1388(%rsp)
   90d03:	00 
   90d04:	48 89 84 24 98 13 00 	mov    %rax,0x1398(%rsp)
   90d0b:	00 
   90d0c:	49 8b 86 61 04 00 00 	mov    0x461(%r14),%rax
   90d13:	41 0f 10 86 51 04 00 	movups 0x451(%r14),%xmm0
   90d1a:	00 
   90d1b:	0f 29 84 24 a0 13 00 	movaps %xmm0,0x13a0(%rsp)
   90d22:	00 
   90d23:	41 0f 10 86 6c 04 00 	movups 0x46c(%r14),%xmm0
   90d2a:	00 
   90d2b:	48 89 84 24 b0 13 00 	mov    %rax,0x13b0(%rsp)
   90d32:	00 
   90d33:	48 89 8c 24 b8 13 00 	mov    %rcx,0x13b8(%rsp)
   90d3a:	00 
   90d3b:	0f 29 84 24 c0 13 00 	movaps %xmm0,0x13c0(%rsp)
   90d42:	00 
   90d43:	41 0f 10 86 7c 04 00 	movups 0x47c(%r14),%xmm0
   90d4a:	00 
   90d4b:	41 0f b7 86 8c 04 00 	movzwl 0x48c(%r14),%eax
   90d52:	00 
   90d53:	0f 29 84 24 d0 13 00 	movaps %xmm0,0x13d0(%rsp)
   90d5a:	00 
   90d5b:	41 0f 10 86 8e 04 00 	movups 0x48e(%r14),%xmm0
   90d62:	00 
   90d63:	48 89 84 24 e0 13 00 	mov    %rax,0x13e0(%rsp)
   90d6a:	00 
   90d6b:	0f 11 84 24 e8 13 00 	movups %xmm0,0x13e8(%rsp)
   90d72:	00 
   90d73:	41 0f 10 86 9e 04 00 	movups 0x49e(%r14),%xmm0
   90d7a:	00 
   90d7b:	41 0f b6 86 ae 04 00 	movzbl 0x4ae(%r14),%eax
   90d82:	00 
   90d83:	0f 11 84 24 f8 13 00 	movups %xmm0,0x13f8(%rsp)
   90d8a:	00 
   90d8b:	41 0f 10 86 af 04 00 	movups 0x4af(%r14),%xmm0
   90d92:	00 
   90d93:	88 84 24 08 14 00 00 	mov    %al,0x1408(%rsp)
   90d9a:	0f 29 84 24 10 14 00 	movaps %xmm0,0x1410(%rsp)
   90da1:	00 
   90da2:	41 0f 10 86 bf 04 00 	movups 0x4bf(%r14),%xmm0
   90da9:	00 
   90daa:	0f 29 84 24 20 14 00 	movaps %xmm0,0x1420(%rsp)
   90db1:	00 
   90db2:	41 0f 10 86 cf 04 00 	movups 0x4cf(%r14),%xmm0
   90db9:	00 
   90dba:	0f 29 84 24 30 14 00 	movaps %xmm0,0x1430(%rsp)
   90dc1:	00 
   90dc2:	49 8b 06             	mov    (%r14),%rax
   90dc5:	48 89 44 24 50       	mov    %rax,0x50(%rsp)
   90dca:	49 8b 86 df 04 00 00 	mov    0x4df(%r14),%rax
   90dd1:	48 89 84 24 40 14 00 	mov    %rax,0x1440(%rsp)
   90dd8:	00 
   90dd9:	48 8b 84 24 b8 00 00 	mov    0xb8(%rsp),%rax
   90de0:	00 
   90de1:	48 83 f8 04          	cmp    $0x4,%rax
   90de5:	0f 87 01 f2 ff ff    	ja     8ffec <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4b9c>
   90deb:	b9 16 00 00 00       	mov    $0x16,%ecx
   90df0:	48 0f a3 c1          	bt     %rax,%rcx
   90df4:	0f 83 f2 f1 ff ff    	jae    8ffec <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4b9c>
   90dfa:	4d 85 e4             	test   %r12,%r12
   90dfd:	0f 88 64 16 00 00    	js     92467 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x7017>
   90e03:	44 89 e3             	mov    %r12d,%ebx
   90e06:	48 8b 4c 24 50       	mov    0x50(%rsp),%rcx
   90e0b:	48 3b 59 08          	cmp    0x8(%rcx),%rbx
   90e0f:	0f 8d 52 16 00 00    	jge    92467 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x7017>
   90e15:	48 8b 4c 24 50       	mov    0x50(%rsp),%rcx
   90e1a:	48 8b 09             	mov    (%rcx),%rcx
   90e1d:	48 69 eb 60 01 00 00 	imul   $0x160,%rbx,%rbp
   90e24:	48 83 3c 29 06       	cmpq   $0x6,(%rcx,%rbp,1)
   90e29:	0f 85 38 16 00 00    	jne    92467 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x7017>
   90e2f:	49 c1 ec 20          	shr    $0x20,%r12
   90e33:	4c 39 64 29 19       	cmp    %r12,0x19(%rcx,%rbp,1)
   90e38:	0f 85 29 16 00 00    	jne    92467 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x7017>
   90e3e:	48 83 f8 01          	cmp    $0x1,%rax
   90e42:	0f 84 1c 03 00 00    	je     91164 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5d14>
   90e48:	48 83 f8 04          	cmp    $0x4,%rax
   90e4c:	0f 85 30 03 00 00    	jne    91182 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5d32>
   90e52:	48 8d bc 24 20 0f 00 	lea    0xf20(%rsp),%rdi
   90e59:	00 
   90e5a:	48 8d b4 24 b8 00 00 	lea    0xb8(%rsp),%rsi
   90e61:	00 
   90e62:	e8 89 5d 03 00       	call   c6bf0 <_Wfl.fl_obj_index$6801bb8d311b3e64>
   90e67:	48 8b 4c 24 50       	mov    0x50(%rsp),%rcx
   90e6c:	48 3b 41 08          	cmp    0x8(%rcx),%rax
   90e70:	0f 83 03 1a 00 00    	jae    92879 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x7429>
   90e76:	48 8b 4c 24 50       	mov    0x50(%rsp),%rcx
   90e7b:	48 8b 09             	mov    (%rcx),%rcx
   90e7e:	48 69 c0 60 01 00 00 	imul   $0x160,%rax,%rax
   90e85:	48 8b 4c 01 11       	mov    0x11(%rcx,%rax,1),%rcx
   90e8a:	e9 74 03 00 00       	jmp    91203 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5db3>
   90e8f:	b8 6b ca eb 85       	mov    $0x85ebca6b,%eax
   90e94:	48 8d 88 4e af 4b 18 	lea    0x184baf4e(%rax),%rcx
   90e9b:	48 85 ed             	test   %rbp,%rbp
   90e9e:	48 0f 44 c8          	cmove  %rax,%rcx
   90ea2:	eb 7b                	jmp    90f1f <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5acf>
   90ea4:	48 89 ea             	mov    %rbp,%rdx
   90ea7:	0f b6 c2             	movzbl %dl,%eax
   90eaa:	35 c5 9d 1c 81       	xor    $0x811c9dc5,%eax
   90eaf:	69 c0 93 01 00 01    	imul   $0x1000193,%eax,%eax
   90eb5:	0f b6 ce             	movzbl %dh,%ecx
   90eb8:	31 c1                	xor    %eax,%ecx
   90eba:	69 c1 93 01 00 01    	imul   $0x1000193,%ecx,%eax
   90ec0:	89 e9                	mov    %ebp,%ecx
   90ec2:	c1 e9 10             	shr    $0x10,%ecx
   90ec5:	0f b6 c9             	movzbl %cl,%ecx
   90ec8:	31 c1                	xor    %eax,%ecx
   90eca:	69 c1 93 01 00 01    	imul   $0x1000193,%ecx,%eax
   90ed0:	89 e9                	mov    %ebp,%ecx
   90ed2:	c1 e9 18             	shr    $0x18,%ecx
   90ed5:	31 c1                	xor    %eax,%ecx
   90ed7:	69 c1 93 01 00 01    	imul   $0x1000193,%ecx,%eax
   90edd:	48 89 e9             	mov    %rbp,%rcx
   90ee0:	48 c1 e9 20          	shr    $0x20,%rcx
   90ee4:	0f b6 c9             	movzbl %cl,%ecx
   90ee7:	31 c1                	xor    %eax,%ecx
   90ee9:	69 c1 93 01 00 01    	imul   $0x1000193,%ecx,%eax
   90eef:	48 89 e9             	mov    %rbp,%rcx
   90ef2:	48 c1 e9 28          	shr    $0x28,%rcx
   90ef6:	0f b6 c9             	movzbl %cl,%ecx
   90ef9:	31 c1                	xor    %eax,%ecx
   90efb:	69 c1 93 01 00 01    	imul   $0x1000193,%ecx,%eax
   90f01:	48 89 e9             	mov    %rbp,%rcx
   90f04:	48 c1 e9 30          	shr    $0x30,%rcx
   90f08:	0f b6 c9             	movzbl %cl,%ecx
   90f0b:	31 c1                	xor    %eax,%ecx
   90f0d:	69 c1 93 01 00 01    	imul   $0x1000193,%ecx,%eax
   90f13:	48 c1 ea 38          	shr    $0x38,%rdx
   90f17:	31 c2                	xor    %eax,%edx
   90f19:	69 ca 93 01 00 01    	imul   $0x1000193,%edx,%ecx
   90f1f:	48 8d bc 24 f0 09 00 	lea    0x9f0(%rsp),%rdi
   90f26:	00 
   90f27:	48 89 de             	mov    %rbx,%rsi
   90f2a:	48 8d 94 24 38 01 00 	lea    0x138(%rsp),%rdx
   90f31:	00 
   90f32:	e8 b9 5f 03 00       	call   c6ef0 <_Wfl.fl_map_probe$234ad4c981fd6647>
   90f37:	84 d2                	test   %dl,%dl
   90f39:	74 50                	je     90f8b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5b3b>
   90f3b:	49 3b 5d 08          	cmp    0x8(%r13),%rbx
   90f3f:	0f 83 27 17 00 00    	jae    9266c <_Wfl.fl_vm_exec$a5081fe92876eb37+0x721c>
   90f45:	49 8b 4d 00          	mov    0x0(%r13),%rcx
   90f49:	4a 8b 54 21 39       	mov    0x39(%rcx,%r12,1),%rdx
   90f4e:	48 3b 42 08          	cmp    0x8(%rdx),%rax
   90f52:	0f 83 14 17 00 00    	jae    9266c <_Wfl.fl_vm_exec$a5081fe92876eb37+0x721c>
   90f58:	48 8b 12             	mov    (%rdx),%rdx
   90f5b:	48 8b 04 c2          	mov    (%rdx,%rax,8),%rax
   90f5f:	48 ff c8             	dec    %rax
   90f62:	0f 80 c7 17 00 00    	jo     9272f <_Wfl.fl_vm_exec$a5081fe92876eb37+0x72df>
   90f68:	4a 8b 4c 21 31       	mov    0x31(%rcx,%r12,1),%rcx
   90f6d:	48 3b 41 08          	cmp    0x8(%rcx),%rax
   90f71:	0f 83 f5 16 00 00    	jae    9266c <_Wfl.fl_vm_exec$a5081fe92876eb37+0x721c>
   90f77:	48 8b 09             	mov    (%rcx),%rcx
   90f7a:	48 c1 e0 06          	shl    $0x6,%rax
   90f7e:	0f 10 44 01 18       	movups 0x18(%rcx,%rax,1),%xmm0
   90f83:	f2 0f 10 4c 01 28    	movsd  0x28(%rcx,%rax,1),%xmm1
   90f89:	eb 2f                	jmp    90fba <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5b6a>
   90f8b:	ba 03 00 00 00       	mov    $0x3,%edx
   90f90:	41 b8 0b 00 00 00    	mov    $0xb,%r8d
   90f96:	4c 89 f7             	mov    %r14,%rdi
   90f99:	48 8d 35 ff 37 f8 ff 	lea    -0x7c801(%rip),%rsi        # 1479f <__abi_tag+0x144a3>
   90fa0:	48 8d 0d 39 3a f8 ff 	lea    -0x7c5c7(%rip),%rcx        # 149e0 <__abi_tag+0x146e4>
   90fa7:	4c 8d 8c 24 48 1f 00 	lea    0x1f48(%rsp),%r9
   90fae:	00 
   90faf:	e8 1c 1b 08 00       	call   112ad0 <_Wfl.fl_raise_bytes.bytesview.4$e8b5fa06fc8adc2d>
   90fb4:	0f 57 c0             	xorps  %xmm0,%xmm0
   90fb7:	0f 57 c9             	xorps  %xmm1,%xmm1
   90fba:	41 80 be 5c 01 00 00 	cmpb   $0x0,0x15c(%r14)
   90fc1:	00 
   90fc2:	0f 85 4f df ff ff    	jne    8ef17 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x3ac7>
   90fc8:	49 8b 86 ac 00 00 00 	mov    0xac(%r14),%rax
   90fcf:	48 3d 00 40 00 00    	cmp    $0x4000,%rax
   90fd5:	0f 8d 74 d5 ff ff    	jge    8e54f <_Wfl.fl_vm_exec$a5081fe92876eb37+0x30ff>
   90fdb:	49 8b 8e a4 00 00 00 	mov    0xa4(%r14),%rcx
   90fe2:	48 3b 41 08          	cmp    0x8(%rcx),%rax
   90fe6:	0f 83 9b 17 00 00    	jae    92787 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x7337>
   90fec:	48 8b 09             	mov    (%rcx),%rcx
   90fef:	48 8d 14 40          	lea    (%rax,%rax,2),%rdx
   90ff3:	0f 11 04 d1          	movups %xmm0,(%rcx,%rdx,8)
   90ff7:	f2 0f 11 4c d1 10    	movsd  %xmm1,0x10(%rcx,%rdx,8)
   90ffd:	e9 0b df ff ff       	jmp    8ef0d <_Wfl.fl_vm_exec$a5081fe92876eb37+0x3abd>
   91002:	bb 0f 00 00 00       	mov    $0xf,%ebx
   91007:	4c 8d 3d 29 39 f8 ff 	lea    -0x7c6d7(%rip),%r15        # 14937 <__abi_tag+0x1463b>
   9100e:	e9 3d eb ff ff       	jmp    8fb50 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4700>
   91013:	49 83 ff 01          	cmp    $0x1,%r15
   91017:	75 1b                	jne    91034 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5be4>
   91019:	b8 6b ca eb 85       	mov    $0x85ebca6b,%eax
   9101e:	48 8d 88 4e af 4b 18 	lea    0x184baf4e(%rax),%rcx
   91025:	48 83 7c 24 28 00    	cmpq   $0x0,0x28(%rsp)
   9102b:	48 0f 44 c8          	cmove  %rax,%rcx
   9102f:	e9 80 00 00 00       	jmp    910b4 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5c64>
   91034:	48 8b 54 24 28       	mov    0x28(%rsp),%rdx
   91039:	0f b6 c2             	movzbl %dl,%eax
   9103c:	35 c5 9d 1c 81       	xor    $0x811c9dc5,%eax
   91041:	69 c0 93 01 00 01    	imul   $0x1000193,%eax,%eax
   91047:	0f b6 ce             	movzbl %dh,%ecx
   9104a:	31 c1                	xor    %eax,%ecx
   9104c:	69 c1 93 01 00 01    	imul   $0x1000193,%ecx,%eax
   91052:	89 d1                	mov    %edx,%ecx
   91054:	c1 e9 10             	shr    $0x10,%ecx
   91057:	0f b6 c9             	movzbl %cl,%ecx
   9105a:	31 c1                	xor    %eax,%ecx
   9105c:	69 c1 93 01 00 01    	imul   $0x1000193,%ecx,%eax
   91062:	89 d1                	mov    %edx,%ecx
   91064:	c1 e9 18             	shr    $0x18,%ecx
   91067:	31 c1                	xor    %eax,%ecx
   91069:	69 c1 93 01 00 01    	imul   $0x1000193,%ecx,%eax
   9106f:	48 89 d1             	mov    %rdx,%rcx
   91072:	48 c1 e9 20          	shr    $0x20,%rcx
   91076:	0f b6 c9             	movzbl %cl,%ecx
   91079:	31 c1                	xor    %eax,%ecx
   9107b:	69 c1 93 01 00 01    	imul   $0x1000193,%ecx,%eax
   91081:	48 89 d1             	mov    %rdx,%rcx
   91084:	48 c1 e9 28          	shr    $0x28,%rcx
   91088:	0f b6 c9             	movzbl %cl,%ecx
   9108b:	31 c1                	xor    %eax,%ecx
   9108d:	69 c1 93 01 00 01    	imul   $0x1000193,%ecx,%eax
   91093:	48 89 d1             	mov    %rdx,%rcx
   91096:	48 c1 e9 30          	shr    $0x30,%rcx
   9109a:	0f b6 c9             	movzbl %cl,%ecx
   9109d:	31 c1                	xor    %eax,%ecx
   9109f:	69 c1 93 01 00 01    	imul   $0x1000193,%ecx,%eax
   910a5:	48 89 d1             	mov    %rdx,%rcx
   910a8:	48 c1 e9 38          	shr    $0x38,%rcx
   910ac:	31 c1                	xor    %eax,%ecx
   910ae:	69 c9 93 01 00 01    	imul   $0x1000193,%ecx,%ecx
   910b4:	48 8d bc 24 c0 04 00 	lea    0x4c0(%rsp),%rdi
   910bb:	00 
   910bc:	48 89 de             	mov    %rbx,%rsi
   910bf:	48 8d 94 24 90 00 00 	lea    0x90(%rsp),%rdx
   910c6:	00 
   910c7:	e8 24 5e 03 00       	call   c6ef0 <_Wfl.fl_map_probe$234ad4c981fd6647>
   910cc:	84 d2                	test   %dl,%dl
   910ce:	0f 85 c3 eb ff ff    	jne    8fc97 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4847>
   910d4:	48 8d bc 24 c0 04 00 	lea    0x4c0(%rsp),%rdi
   910db:	00 
   910dc:	48 8d b4 24 98 01 00 	lea    0x198(%rsp),%rsi
   910e3:	00 
   910e4:	48 8b 54 24 28       	mov    0x28(%rsp),%rdx
   910e9:	e8 92 e3 07 00       	call   10f480 <_Wfl.fl_missing_global$adcb0e841291d8c3>
   910ee:	ba 04 00 00 00       	mov    $0x4,%edx
   910f3:	4c 89 f7             	mov    %r14,%rdi
   910f6:	48 8d 35 d3 d2 f8 ff 	lea    -0x72d2d(%rip),%rsi        # 1e3d0 <__abi_tag+0x1e0d4>
   910fd:	48 89 c1             	mov    %rax,%rcx
   91100:	4c 8d 84 24 d8 1f 00 	lea    0x1fd8(%rsp),%r8
   91107:	00 
   91108:	e8 e3 62 02 00       	call   b73f0 <_Wfl.fl_raise_bytes$05a094d3779625bf>
   9110d:	0f 57 c0             	xorps  %xmm0,%xmm0
   91110:	31 c0                	xor    %eax,%eax
   91112:	41 80 be 5c 01 00 00 	cmpb   $0x0,0x15c(%r14)
   91119:	00 
   9111a:	0f 85 f7 dd ff ff    	jne    8ef17 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x3ac7>
   91120:	49 8b 8e ac 00 00 00 	mov    0xac(%r14),%rcx
   91127:	48 81 f9 00 40 00 00 	cmp    $0x4000,%rcx
   9112e:	0f 8d 1b d4 ff ff    	jge    8e54f <_Wfl.fl_vm_exec$a5081fe92876eb37+0x30ff>
   91134:	49 8b 96 a4 00 00 00 	mov    0xa4(%r14),%rdx
   9113b:	48 3b 4a 08          	cmp    0x8(%rdx),%rcx
   9113f:	0f 83 d1 16 00 00    	jae    92816 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x73c6>
   91145:	48 8b 12             	mov    (%rdx),%rdx
   91148:	48 8d 34 49          	lea    (%rcx,%rcx,2),%rsi
   9114c:	0f 11 04 f2          	movups %xmm0,(%rdx,%rsi,8)
   91150:	48 89 44 f2 10       	mov    %rax,0x10(%rdx,%rsi,8)
   91155:	48 ff c1             	inc    %rcx
   91158:	49 89 8e ac 00 00 00 	mov    %rcx,0xac(%r14)
   9115f:	e9 b3 dd ff ff       	jmp    8ef17 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x3ac7>
   91164:	b8 6b ca eb 85       	mov    $0x85ebca6b,%eax
   91169:	48 8d 88 4e af 4b 18 	lea    0x184baf4e(%rax),%rcx
   91170:	48 83 bc 24 c0 00 00 	cmpq   $0x0,0xc0(%rsp)
   91177:	00 00 
   91179:	48 0f 44 c8          	cmove  %rax,%rcx
   9117d:	e9 81 00 00 00       	jmp    91203 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5db3>
   91182:	48 8b 84 24 c0 00 00 	mov    0xc0(%rsp),%rax
   91189:	00 
   9118a:	0f b6 c8             	movzbl %al,%ecx
   9118d:	81 f1 c5 9d 1c 81    	xor    $0x811c9dc5,%ecx
   91193:	69 c9 93 01 00 01    	imul   $0x1000193,%ecx,%ecx
   91199:	0f b6 d4             	movzbl %ah,%edx
   9119c:	31 ca                	xor    %ecx,%edx
   9119e:	69 ca 93 01 00 01    	imul   $0x1000193,%edx,%ecx
   911a4:	89 c2                	mov    %eax,%edx
   911a6:	c1 ea 10             	shr    $0x10,%edx
   911a9:	0f b6 d2             	movzbl %dl,%edx
   911ac:	31 ca                	xor    %ecx,%edx
   911ae:	69 ca 93 01 00 01    	imul   $0x1000193,%edx,%ecx
   911b4:	89 c2                	mov    %eax,%edx
   911b6:	c1 ea 18             	shr    $0x18,%edx
   911b9:	31 ca                	xor    %ecx,%edx
   911bb:	69 ca 93 01 00 01    	imul   $0x1000193,%edx,%ecx
   911c1:	48 89 c2             	mov    %rax,%rdx
   911c4:	48 c1 ea 20          	shr    $0x20,%rdx
   911c8:	0f b6 d2             	movzbl %dl,%edx
   911cb:	31 ca                	xor    %ecx,%edx
   911cd:	69 ca 93 01 00 01    	imul   $0x1000193,%edx,%ecx
   911d3:	48 89 c2             	mov    %rax,%rdx
   911d6:	48 c1 ea 28          	shr    $0x28,%rdx
   911da:	0f b6 d2             	movzbl %dl,%edx
   911dd:	31 ca                	xor    %ecx,%edx
   911df:	69 ca 93 01 00 01    	imul   $0x1000193,%edx,%ecx
   911e5:	48 89 c2             	mov    %rax,%rdx
   911e8:	48 c1 ea 30          	shr    $0x30,%rdx
   911ec:	0f b6 d2             	movzbl %dl,%edx
   911ef:	31 ca                	xor    %ecx,%edx
   911f1:	69 ca 93 01 00 01    	imul   $0x1000193,%edx,%ecx
   911f7:	48 c1 e8 38          	shr    $0x38,%rax
   911fb:	31 c8                	xor    %ecx,%eax
   911fd:	69 c8 93 01 00 01    	imul   $0x1000193,%eax,%ecx
   91203:	48 8d bc 24 20 0f 00 	lea    0xf20(%rsp),%rdi
   9120a:	00 
   9120b:	48 89 de             	mov    %rbx,%rsi
   9120e:	48 8d 94 24 b8 00 00 	lea    0xb8(%rsp),%rdx
   91215:	00 
   91216:	e8 d5 5c 03 00       	call   c6ef0 <_Wfl.fl_map_probe$234ad4c981fd6647>
   9121b:	84 d2                	test   %dl,%dl
   9121d:	0f 84 c9 ed ff ff    	je     8ffec <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4b9c>
   91223:	48 8b 4c 24 50       	mov    0x50(%rsp),%rcx
   91228:	48 3b 59 08          	cmp    0x8(%rcx),%rbx
   9122c:	0f 83 47 16 00 00    	jae    92879 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x7429>
   91232:	48 8b 4c 24 50       	mov    0x50(%rsp),%rcx
   91237:	48 8b 09             	mov    (%rcx),%rcx
   9123a:	48 8b 54 29 39       	mov    0x39(%rcx,%rbp,1),%rdx
   9123f:	48 3b 42 08          	cmp    0x8(%rdx),%rax
   91243:	0f 83 30 16 00 00    	jae    92879 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x7429>
   91249:	48 8b 12             	mov    (%rdx),%rdx
   9124c:	48 8b 04 c2          	mov    (%rdx,%rax,8),%rax
   91250:	48 ff c8             	dec    %rax
   91253:	0f 80 df 12 00 00    	jo     92538 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x70e8>
   91259:	48 8b 54 29 31       	mov    0x31(%rcx,%rbp,1),%rdx
   9125e:	48 3b 42 08          	cmp    0x8(%rdx),%rax
   91262:	0f 83 11 16 00 00    	jae    92879 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x7429>
   91268:	49 8b 8e ac 00 00 00 	mov    0xac(%r14),%rcx
   9126f:	48 81 f9 ff 3f 00 00 	cmp    $0x3fff,%rcx
   91276:	0f 8f d3 d2 ff ff    	jg     8e54f <_Wfl.fl_vm_exec$a5081fe92876eb37+0x30ff>
   9127c:	49 8b b6 a4 00 00 00 	mov    0xa4(%r14),%rsi
   91283:	48 3b 4e 08          	cmp    0x8(%rsi),%rcx
   91287:	0f 83 b2 16 00 00    	jae    9293f <_Wfl.fl_vm_exec$a5081fe92876eb37+0x74ef>
   9128d:	48 8b 12             	mov    (%rdx),%rdx
   91290:	48 c1 e0 06          	shl    $0x6,%rax
   91294:	0f 10 44 02 18       	movups 0x18(%rdx,%rax,1),%xmm0
   91299:	f2 0f 10 4c 02 28    	movsd  0x28(%rdx,%rax,1),%xmm1
   9129f:	48 8b 06             	mov    (%rsi),%rax
   912a2:	48 8d 14 49          	lea    (%rcx,%rcx,2),%rdx
   912a6:	0f 11 04 d0          	movups %xmm0,(%rax,%rdx,8)
   912aa:	f2 0f 11 4c d0 10    	movsd  %xmm1,0x10(%rax,%rdx,8)
   912b0:	e9 a0 fe ff ff       	jmp    91155 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5d05>
   912b5:	0f 57 c0             	xorps  %xmm0,%xmm0
   912b8:	48 8b 84 24 88 00 00 	mov    0x88(%rsp),%rax
   912bf:	00 
   912c0:	0f 11 00             	movups %xmm0,(%rax)
   912c3:	48 c7 40 10 00 00 00 	movq   $0x0,0x10(%rax)
   912ca:	00 
   912cb:	eb 36                	jmp    91303 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5eb3>
   912cd:	49 89 96 b5 01 00 00 	mov    %rdx,0x1b5(%r14)
   912d4:	4c 89 f7             	mov    %r14,%rdi
   912d7:	e8 f4 52 02 00       	call   b65d0 <_Wfl.fl_trace_attach$26a4ad1c7c1e1b81>
   912dc:	eb 07                	jmp    912e5 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5e95>
   912de:	49 89 96 b5 01 00 00 	mov    %rdx,0x1b5(%r14)
   912e5:	49 8b 86 6d 01 00 00 	mov    0x16d(%r14),%rax
   912ec:	41 0f 10 86 5d 01 00 	movups 0x15d(%r14),%xmm0
   912f3:	00 
   912f4:	48 8b 8c 24 88 00 00 	mov    0x88(%rsp),%rcx
   912fb:	00 
   912fc:	0f 11 01             	movups %xmm0,(%rcx)
   912ff:	48 89 41 10          	mov    %rax,0x10(%rcx)
   91303:	48 81 c4 48 20 00 00 	add    $0x2048,%rsp
   9130a:	5b                   	pop    %rbx
   9130b:	41 5c                	pop    %r12
   9130d:	41 5d                	pop    %r13
   9130f:	41 5e                	pop    %r14
   91311:	41 5f                	pop    %r15
   91313:	5d                   	pop    %rbp
   91314:	c3                   	ret
   91315:	48 8d 35 56 34 f8 ff 	lea    -0x7cbaa(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   9131c:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91321:	b9 68 02 00 00       	mov    $0x268,%ecx
   91326:	41 b8 15 00 00 00    	mov    $0x15,%r8d
   9132c:	bf 03 00 00 00       	mov    $0x3,%edi
   91331:	e8 ea 60 08 00       	call   117420 <__wolf_rt_trap_at>
   91336:	48 8d 35 35 34 f8 ff 	lea    -0x7cbcb(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   9133d:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91342:	b9 f9 01 00 00       	mov    $0x1f9,%ecx
   91347:	eb 4d                	jmp    91396 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5f46>
   91349:	48 8d 35 22 34 f8 ff 	lea    -0x7cbde(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91350:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91355:	b9 9e 03 00 00       	mov    $0x39e,%ecx
   9135a:	eb 3a                	jmp    91396 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5f46>
   9135c:	48 8d 35 0f 34 f8 ff 	lea    -0x7cbf1(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91363:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91368:	b9 b5 03 00 00       	mov    $0x3b5,%ecx
   9136d:	eb 27                	jmp    91396 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5f46>
   9136f:	48 8d 35 fc 33 f8 ff 	lea    -0x7cc04(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91376:	ba 2d 00 00 00       	mov    $0x2d,%edx
   9137b:	b9 1a 02 00 00       	mov    $0x21a,%ecx
   91380:	e9 02 02 00 00       	jmp    91587 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6137>
   91385:	48 8d 35 e6 33 f8 ff 	lea    -0x7cc1a(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   9138c:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91391:	b9 fd 01 00 00       	mov    $0x1fd,%ecx
   91396:	41 b8 0d 00 00 00    	mov    $0xd,%r8d
   9139c:	bf 01 00 00 00       	mov    $0x1,%edi
   913a1:	e8 7a 60 08 00       	call   117420 <__wolf_rt_trap_at>
   913a6:	48 8d 35 c5 33 f8 ff 	lea    -0x7cc3b(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   913ad:	ba 2d 00 00 00       	mov    $0x2d,%edx
   913b2:	b9 0b 02 00 00       	mov    $0x20b,%ecx
   913b7:	41 b8 33 00 00 00    	mov    $0x33,%r8d
   913bd:	bf 01 00 00 00       	mov    $0x1,%edi
   913c2:	e8 59 60 08 00       	call   117420 <__wolf_rt_trap_at>
   913c7:	48 8d 35 a4 33 f8 ff 	lea    -0x7cc5c(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   913ce:	ba 2d 00 00 00       	mov    $0x2d,%edx
   913d3:	b9 1f 02 00 00       	mov    $0x21f,%ecx
   913d8:	e9 aa 01 00 00       	jmp    91587 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6137>
   913dd:	48 8d 35 8e 33 f8 ff 	lea    -0x7cc72(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   913e4:	ba 2d 00 00 00       	mov    $0x2d,%edx
   913e9:	b9 0f 02 00 00       	mov    $0x20f,%ecx
   913ee:	41 b8 11 00 00 00    	mov    $0x11,%r8d
   913f4:	bf 01 00 00 00       	mov    $0x1,%edi
   913f9:	e8 22 60 08 00       	call   117420 <__wolf_rt_trap_at>
   913fe:	48 8d 35 6d 33 f8 ff 	lea    -0x7cc93(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91405:	ba 2d 00 00 00       	mov    $0x2d,%edx
   9140a:	b9 24 02 00 00       	mov    $0x224,%ecx
   9140f:	e9 73 01 00 00       	jmp    91587 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6137>
   91414:	48 8d 35 57 33 f8 ff 	lea    -0x7cca9(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   9141b:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91420:	b9 46 02 00 00       	mov    $0x246,%ecx
   91425:	e9 9f 01 00 00       	jmp    915c9 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6179>
   9142a:	48 8d 35 41 33 f8 ff 	lea    -0x7ccbf(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91431:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91436:	b9 33 02 00 00       	mov    $0x233,%ecx
   9143b:	e9 89 01 00 00       	jmp    915c9 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6179>
   91440:	48 8d 35 2b 33 f8 ff 	lea    -0x7ccd5(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91447:	ba 2d 00 00 00       	mov    $0x2d,%edx
   9144c:	b9 42 02 00 00       	mov    $0x242,%ecx
   91451:	41 b8 66 00 00 00    	mov    $0x66,%r8d
   91457:	bf 01 00 00 00       	mov    $0x1,%edi
   9145c:	e8 bf 5f 08 00       	call   117420 <__wolf_rt_trap_at>
   91461:	48 8d 35 0a 33 f8 ff 	lea    -0x7ccf6(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91468:	ba 2d 00 00 00       	mov    $0x2d,%edx
   9146d:	b9 44 02 00 00       	mov    $0x244,%ecx
   91472:	41 b8 18 00 00 00    	mov    $0x18,%r8d
   91478:	bf 01 00 00 00       	mov    $0x1,%edi
   9147d:	e8 9e 5f 08 00       	call   117420 <__wolf_rt_trap_at>
   91482:	48 8d 35 e9 32 f8 ff 	lea    -0x7cd17(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91489:	ba 2d 00 00 00       	mov    $0x2d,%edx
   9148e:	b9 4b 02 00 00       	mov    $0x24b,%ecx
   91493:	e9 31 01 00 00       	jmp    915c9 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6179>
   91498:	48 8d 35 d3 32 f8 ff 	lea    -0x7cd2d(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   9149f:	ba 2d 00 00 00       	mov    $0x2d,%edx
   914a4:	b9 3f 02 00 00       	mov    $0x23f,%ecx
   914a9:	e9 1b 01 00 00       	jmp    915c9 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6179>
   914ae:	48 8d 35 bd 32 f8 ff 	lea    -0x7cd43(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   914b5:	ba 2d 00 00 00       	mov    $0x2d,%edx
   914ba:	b9 3b 02 00 00       	mov    $0x23b,%ecx
   914bf:	e9 05 01 00 00       	jmp    915c9 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6179>
   914c4:	48 8d 35 a7 32 f8 ff 	lea    -0x7cd59(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   914cb:	ba 2d 00 00 00       	mov    $0x2d,%edx
   914d0:	b9 43 02 00 00       	mov    $0x243,%ecx
   914d5:	41 b8 3d 00 00 00    	mov    $0x3d,%r8d
   914db:	bf 01 00 00 00       	mov    $0x1,%edi
   914e0:	e8 3b 5f 08 00       	call   117420 <__wolf_rt_trap_at>
   914e5:	48 8d 35 86 32 f8 ff 	lea    -0x7cd7a(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   914ec:	ba 2d 00 00 00       	mov    $0x2d,%edx
   914f1:	b9 47 02 00 00       	mov    $0x247,%ecx
   914f6:	e9 8c 00 00 00       	jmp    91587 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6137>
   914fb:	48 8d 35 70 32 f8 ff 	lea    -0x7cd90(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91502:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91507:	b9 4c 02 00 00       	mov    $0x24c,%ecx
   9150c:	41 b8 63 00 00 00    	mov    $0x63,%r8d
   91512:	bf 01 00 00 00       	mov    $0x1,%edi
   91517:	e8 04 5f 08 00       	call   117420 <__wolf_rt_trap_at>
   9151c:	48 8d 35 4f 32 f8 ff 	lea    -0x7cdb1(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91523:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91528:	b9 43 02 00 00       	mov    $0x243,%ecx
   9152d:	41 b8 7e 00 00 00    	mov    $0x7e,%r8d
   91533:	bf 01 00 00 00       	mov    $0x1,%edi
   91538:	e8 e3 5e 08 00       	call   117420 <__wolf_rt_trap_at>
   9153d:	48 8d 35 2e 32 f8 ff 	lea    -0x7cdd2(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91544:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91549:	b9 4d 02 00 00       	mov    $0x24d,%ecx
   9154e:	eb 37                	jmp    91587 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6137>
   91550:	48 8d 35 1b 32 f8 ff 	lea    -0x7cde5(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91557:	ba 2d 00 00 00       	mov    $0x2d,%edx
   9155c:	b9 50 02 00 00       	mov    $0x250,%ecx
   91561:	eb 66                	jmp    915c9 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6179>
   91563:	48 8d 35 08 32 f8 ff 	lea    -0x7cdf8(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   9156a:	ba 2d 00 00 00       	mov    $0x2d,%edx
   9156f:	b9 61 02 00 00       	mov    $0x261,%ecx
   91574:	eb 53                	jmp    915c9 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6179>
   91576:	48 8d 35 f5 31 f8 ff 	lea    -0x7ce0b(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   9157d:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91582:	b9 62 02 00 00       	mov    $0x262,%ecx
   91587:	41 b8 15 00 00 00    	mov    $0x15,%r8d
   9158d:	bf 01 00 00 00       	mov    $0x1,%edi
   91592:	e8 89 5e 08 00       	call   117420 <__wolf_rt_trap_at>
   91597:	48 8d 35 d4 31 f8 ff 	lea    -0x7ce2c(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   9159e:	ba 2d 00 00 00       	mov    $0x2d,%edx
   915a3:	b9 5c 02 00 00       	mov    $0x25c,%ecx
   915a8:	41 b8 67 00 00 00    	mov    $0x67,%r8d
   915ae:	bf 01 00 00 00       	mov    $0x1,%edi
   915b3:	e8 68 5e 08 00       	call   117420 <__wolf_rt_trap_at>
   915b8:	48 8d 35 b3 31 f8 ff 	lea    -0x7ce4d(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   915bf:	ba 2d 00 00 00       	mov    $0x2d,%edx
   915c4:	b9 66 02 00 00       	mov    $0x266,%ecx
   915c9:	41 b8 3a 00 00 00    	mov    $0x3a,%r8d
   915cf:	bf 01 00 00 00       	mov    $0x1,%edi
   915d4:	e8 47 5e 08 00       	call   117420 <__wolf_rt_trap_at>
   915d9:	48 8b 84 24 88 00 00 	mov    0x88(%rsp),%rax
   915e0:	00 
   915e1:	0f 28 44 24 50       	movaps 0x50(%rsp),%xmm0
   915e6:	0f 11 00             	movups %xmm0,(%rax)
   915e9:	48 89 58 10          	mov    %rbx,0x10(%rax)
   915ed:	e9 11 fd ff ff       	jmp    91303 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5eb3>
   915f2:	48 8d 35 79 31 f8 ff 	lea    -0x7ce87(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   915f9:	ba 2d 00 00 00       	mov    $0x2d,%edx
   915fe:	b9 85 02 00 00       	mov    $0x285,%ecx
   91603:	e9 04 ff ff ff       	jmp    9150c <_Wfl.fl_vm_exec$a5081fe92876eb37+0x60bc>
   91608:	48 8d 35 63 31 f8 ff 	lea    -0x7ce9d(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   9160f:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91614:	b9 86 02 00 00       	mov    $0x286,%ecx
   91619:	e9 ee fe ff ff       	jmp    9150c <_Wfl.fl_vm_exec$a5081fe92876eb37+0x60bc>
   9161e:	48 8d 35 4d 31 f8 ff 	lea    -0x7ceb3(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91625:	ba 2d 00 00 00       	mov    $0x2d,%edx
   9162a:	b9 02 02 00 00       	mov    $0x202,%ecx
   9162f:	eb 11                	jmp    91642 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x61f2>
   91631:	48 8d 35 3a 31 f8 ff 	lea    -0x7cec6(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91638:	ba 2d 00 00 00       	mov    $0x2d,%edx
   9163d:	b9 00 02 00 00       	mov    $0x200,%ecx
   91642:	41 b8 11 00 00 00    	mov    $0x11,%r8d
   91648:	bf 04 00 00 00       	mov    $0x4,%edi
   9164d:	e8 ce 5d 08 00       	call   117420 <__wolf_rt_trap_at>
   91652:	48 8d 35 19 31 f8 ff 	lea    -0x7cee7(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91659:	ba 2d 00 00 00       	mov    $0x2d,%edx
   9165e:	b9 0b 02 00 00       	mov    $0x20b,%ecx
   91663:	41 b8 4e 00 00 00    	mov    $0x4e,%r8d
   91669:	bf 03 00 00 00       	mov    $0x3,%edi
   9166e:	e8 ad 5d 08 00       	call   117420 <__wolf_rt_trap_at>
   91673:	48 8d 35 f8 30 f8 ff 	lea    -0x7cf08(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   9167a:	ba 2d 00 00 00       	mov    $0x2d,%edx
   9167f:	b9 1a 02 00 00       	mov    $0x21a,%ecx
   91684:	e9 9d fc ff ff       	jmp    91326 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5ed6>
   91689:	48 8d 35 e2 30 f8 ff 	lea    -0x7cf1e(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91690:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91695:	b9 9e 03 00 00       	mov    $0x39e,%ecx
   9169a:	e9 87 01 00 00       	jmp    91826 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x63d6>
   9169f:	48 8d 35 cc 30 f8 ff 	lea    -0x7cf34(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   916a6:	ba 2d 00 00 00       	mov    $0x2d,%edx
   916ab:	b9 b5 03 00 00       	mov    $0x3b5,%ecx
   916b0:	e9 71 01 00 00       	jmp    91826 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x63d6>
   916b5:	48 8d 35 b6 30 f8 ff 	lea    -0x7cf4a(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   916bc:	ba 2d 00 00 00       	mov    $0x2d,%edx
   916c1:	b9 0b 02 00 00       	mov    $0x20b,%ecx
   916c6:	41 b8 18 00 00 00    	mov    $0x18,%r8d
   916cc:	bf 03 00 00 00       	mov    $0x3,%edi
   916d1:	e8 4a 5d 08 00       	call   117420 <__wolf_rt_trap_at>
   916d6:	48 8d 35 95 30 f8 ff 	lea    -0x7cf6b(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   916dd:	ba 2d 00 00 00       	mov    $0x2d,%edx
   916e2:	b9 07 02 00 00       	mov    $0x207,%ecx
   916e7:	e9 3a 01 00 00       	jmp    91826 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x63d6>
   916ec:	48 8d 35 7f 30 f8 ff 	lea    -0x7cf81(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   916f3:	ba 2d 00 00 00       	mov    $0x2d,%edx
   916f8:	b9 8b 02 00 00       	mov    $0x28b,%ecx
   916fd:	e9 85 fe ff ff       	jmp    91587 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6137>
   91702:	48 8d 35 69 30 f8 ff 	lea    -0x7cf97(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91709:	ba 2d 00 00 00       	mov    $0x2d,%edx
   9170e:	b9 94 02 00 00       	mov    $0x294,%ecx
   91713:	e9 8f 02 00 00       	jmp    919a7 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6557>
   91718:	48 8d 35 53 30 f8 ff 	lea    -0x7cfad(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   9171f:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91724:	b9 40 02 00 00       	mov    $0x240,%ecx
   91729:	41 b8 70 00 00 00    	mov    $0x70,%r8d
   9172f:	bf 03 00 00 00       	mov    $0x3,%edi
   91734:	e8 e7 5c 08 00       	call   117420 <__wolf_rt_trap_at>
   91739:	48 8d 35 32 30 f8 ff 	lea    -0x7cfce(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91740:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91745:	b9 9d 03 00 00       	mov    $0x39d,%ecx
   9174a:	41 b8 37 00 00 00    	mov    $0x37,%r8d
   91750:	bf 03 00 00 00       	mov    $0x3,%edi
   91755:	e8 c6 5c 08 00       	call   117420 <__wolf_rt_trap_at>
   9175a:	48 8d 35 11 30 f8 ff 	lea    -0x7cfef(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91761:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91766:	b9 b4 03 00 00       	mov    $0x3b4,%ecx
   9176b:	41 b8 53 00 00 00    	mov    $0x53,%r8d
   91771:	bf 03 00 00 00       	mov    $0x3,%edi
   91776:	e8 a5 5c 08 00       	call   117420 <__wolf_rt_trap_at>
   9177b:	48 8d 35 f0 2f f8 ff 	lea    -0x7d010(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91782:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91787:	b9 d5 02 00 00       	mov    $0x2d5,%ecx
   9178c:	e9 16 02 00 00       	jmp    919a7 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6557>
   91791:	48 8d 35 da 2f f8 ff 	lea    -0x7d026(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91798:	ba 2d 00 00 00       	mov    $0x2d,%edx
   9179d:	b9 1f 02 00 00       	mov    $0x21f,%ecx
   917a2:	e9 7f fb ff ff       	jmp    91326 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5ed6>
   917a7:	48 8d 35 c4 2f f8 ff 	lea    -0x7d03c(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   917ae:	ba 2d 00 00 00       	mov    $0x2d,%edx
   917b3:	b9 ff 01 00 00       	mov    $0x1ff,%ecx
   917b8:	e9 8a 00 00 00       	jmp    91847 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x63f7>
   917bd:	48 8d 35 ae 2f f8 ff 	lea    -0x7d052(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   917c4:	ba 2d 00 00 00       	mov    $0x2d,%edx
   917c9:	b9 df 02 00 00       	mov    $0x2df,%ecx
   917ce:	e9 d4 01 00 00       	jmp    919a7 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6557>
   917d3:	48 8d 35 98 2f f8 ff 	lea    -0x7d068(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   917da:	ba 2d 00 00 00       	mov    $0x2d,%edx
   917df:	b9 24 02 00 00       	mov    $0x224,%ecx
   917e4:	e9 3d fb ff ff       	jmp    91326 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5ed6>
   917e9:	48 8d 35 82 2f f8 ff 	lea    -0x7d07e(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   917f0:	ba 2d 00 00 00       	mov    $0x2d,%edx
   917f5:	b9 96 02 00 00       	mov    $0x296,%ecx
   917fa:	e9 0d fd ff ff       	jmp    9150c <_Wfl.fl_vm_exec$a5081fe92876eb37+0x60bc>
   917ff:	48 8d 35 6c 2f f8 ff 	lea    -0x7d094(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91806:	ba 2d 00 00 00       	mov    $0x2d,%edx
   9180b:	b9 97 02 00 00       	mov    $0x297,%ecx
   91810:	e9 f7 fc ff ff       	jmp    9150c <_Wfl.fl_vm_exec$a5081fe92876eb37+0x60bc>
   91815:	48 8d 35 56 2f f8 ff 	lea    -0x7d0aa(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   9181c:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91821:	b9 08 02 00 00       	mov    $0x208,%ecx
   91826:	41 b8 0d 00 00 00    	mov    $0xd,%r8d
   9182c:	bf 03 00 00 00       	mov    $0x3,%edi
   91831:	e8 ea 5b 08 00       	call   117420 <__wolf_rt_trap_at>
   91836:	48 8d 35 35 2f f8 ff 	lea    -0x7d0cb(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   9183d:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91842:	b9 0f 02 00 00       	mov    $0x20f,%ecx
   91847:	41 b8 11 00 00 00    	mov    $0x11,%r8d
   9184d:	bf 03 00 00 00       	mov    $0x3,%edi
   91852:	e8 c9 5b 08 00       	call   117420 <__wolf_rt_trap_at>
   91857:	48 8d 35 14 2f f8 ff 	lea    -0x7d0ec(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   9185e:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91863:	b9 c6 02 00 00       	mov    $0x2c6,%ecx
   91868:	41 b8 42 00 00 00    	mov    $0x42,%r8d
   9186e:	bf 01 00 00 00       	mov    $0x1,%edi
   91873:	e8 a8 5b 08 00       	call   117420 <__wolf_rt_trap_at>
   91878:	48 8d 35 f3 2e f8 ff 	lea    -0x7d10d(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   9187f:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91884:	b9 c7 02 00 00       	mov    $0x2c7,%ecx
   91889:	41 b8 3f 00 00 00    	mov    $0x3f,%r8d
   9188f:	bf 01 00 00 00       	mov    $0x1,%edi
   91894:	e8 87 5b 08 00       	call   117420 <__wolf_rt_trap_at>
   91899:	48 8d 35 d2 2e f8 ff 	lea    -0x7d12e(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   918a0:	ba 2d 00 00 00       	mov    $0x2d,%edx
   918a5:	b9 33 02 00 00       	mov    $0x233,%ecx
   918aa:	e9 f7 03 00 00       	jmp    91ca6 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6856>
   918af:	48 8d 35 bc 2e f8 ff 	lea    -0x7d144(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   918b6:	ba 2d 00 00 00       	mov    $0x2d,%edx
   918bb:	b9 9c 02 00 00       	mov    $0x29c,%ecx
   918c0:	e9 04 fd ff ff       	jmp    915c9 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6179>
   918c5:	48 8d 35 a6 2e f8 ff 	lea    -0x7d15a(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   918cc:	ba 2d 00 00 00       	mov    $0x2d,%edx
   918d1:	b9 a7 02 00 00       	mov    $0x2a7,%ecx
   918d6:	e9 ac fc ff ff       	jmp    91587 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6137>
   918db:	48 8d 35 90 2e f8 ff 	lea    -0x7d170(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   918e2:	ba 2d 00 00 00       	mov    $0x2d,%edx
   918e7:	b9 c9 02 00 00       	mov    $0x2c9,%ecx
   918ec:	41 b8 1d 00 00 00    	mov    $0x1d,%r8d
   918f2:	bf 01 00 00 00       	mov    $0x1,%edi
   918f7:	e8 24 5b 08 00       	call   117420 <__wolf_rt_trap_at>
   918fc:	48 8d 35 6f 2e f8 ff 	lea    -0x7d191(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91903:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91908:	b9 52 02 00 00       	mov    $0x252,%ecx
   9190d:	41 b8 15 00 00 00    	mov    $0x15,%r8d
   91913:	bf 04 00 00 00       	mov    $0x4,%edi
   91918:	e8 03 5b 08 00       	call   117420 <__wolf_rt_trap_at>
   9191d:	48 8d 35 4e 2e f8 ff 	lea    -0x7d1b2(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91924:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91929:	b9 43 02 00 00       	mov    $0x243,%ecx
   9192e:	41 b8 58 00 00 00    	mov    $0x58,%r8d
   91934:	bf 03 00 00 00       	mov    $0x3,%edi
   91939:	e8 e2 5a 08 00       	call   117420 <__wolf_rt_trap_at>
   9193e:	48 8d 35 2d 2e f8 ff 	lea    -0x7d1d3(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91945:	ba 2d 00 00 00       	mov    $0x2d,%edx
   9194a:	b9 3f 02 00 00       	mov    $0x23f,%ecx
   9194f:	e9 52 03 00 00       	jmp    91ca6 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6856>
   91954:	48 8d 35 17 2e f8 ff 	lea    -0x7d1e9(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   9195b:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91960:	b9 4b 02 00 00       	mov    $0x24b,%ecx
   91965:	e9 3c 03 00 00       	jmp    91ca6 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6856>
   9196a:	48 8d 35 01 2e f8 ff 	lea    -0x7d1ff(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91971:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91976:	b9 46 02 00 00       	mov    $0x246,%ecx
   9197b:	e9 26 03 00 00       	jmp    91ca6 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6856>
   91980:	48 8d 35 eb 2d f8 ff 	lea    -0x7d215(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91987:	ba 2d 00 00 00       	mov    $0x2d,%edx
   9198c:	b9 3b 02 00 00       	mov    $0x23b,%ecx
   91991:	e9 10 03 00 00       	jmp    91ca6 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6856>
   91996:	48 8d 35 d5 2d f8 ff 	lea    -0x7d22b(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   9199d:	ba 2d 00 00 00       	mov    $0x2d,%edx
   919a2:	b9 e0 02 00 00       	mov    $0x2e0,%ecx
   919a7:	41 b8 19 00 00 00    	mov    $0x19,%r8d
   919ad:	bf 01 00 00 00       	mov    $0x1,%edi
   919b2:	e8 69 5a 08 00       	call   117420 <__wolf_rt_trap_at>
   919b7:	48 8d 35 b4 2d f8 ff 	lea    -0x7d24c(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   919be:	ba 2d 00 00 00       	mov    $0x2d,%edx
   919c3:	b9 a3 02 00 00       	mov    $0x2a3,%ecx
   919c8:	41 b8 4a 00 00 00    	mov    $0x4a,%r8d
   919ce:	bf 01 00 00 00       	mov    $0x1,%edi
   919d3:	e8 48 5a 08 00       	call   117420 <__wolf_rt_trap_at>
   919d8:	48 8d 35 93 2d f8 ff 	lea    -0x7d26d(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   919df:	ba 2d 00 00 00       	mov    $0x2d,%edx
   919e4:	b9 9f 02 00 00       	mov    $0x29f,%ecx
   919e9:	e9 ba fb ff ff       	jmp    915a8 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6158>
   919ee:	48 8d 35 7d 2d f8 ff 	lea    -0x7d283(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   919f5:	ba 2d 00 00 00       	mov    $0x2d,%edx
   919fa:	b9 a6 02 00 00       	mov    $0x2a6,%ecx
   919ff:	e9 c5 fb ff ff       	jmp    915c9 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6179>
   91a04:	48 8d 35 67 2d f8 ff 	lea    -0x7d299(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91a0b:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91a10:	b9 a2 02 00 00       	mov    $0x2a2,%ecx
   91a15:	41 b8 2f 00 00 00    	mov    $0x2f,%r8d
   91a1b:	bf 01 00 00 00       	mov    $0x1,%edi
   91a20:	e8 fb 59 08 00       	call   117420 <__wolf_rt_trap_at>
   91a25:	48 8d 35 46 2d f8 ff 	lea    -0x7d2ba(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91a2c:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91a31:	b9 4b 02 00 00       	mov    $0x24b,%ecx
   91a36:	e9 7e 03 00 00       	jmp    91db9 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6969>
   91a3b:	48 8d 35 30 2d f8 ff 	lea    -0x7d2d0(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91a42:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91a47:	b9 33 02 00 00       	mov    $0x233,%ecx
   91a4c:	e9 68 03 00 00       	jmp    91db9 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6969>
   91a51:	48 8d 35 1a 2d f8 ff 	lea    -0x7d2e6(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91a58:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91a5d:	b9 43 02 00 00       	mov    $0x243,%ecx
   91a62:	41 b8 22 00 00 00    	mov    $0x22,%r8d
   91a68:	bf 03 00 00 00       	mov    $0x3,%edi
   91a6d:	e8 ae 59 08 00       	call   117420 <__wolf_rt_trap_at>
   91a72:	48 8d 35 f9 2c f8 ff 	lea    -0x7d307(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91a79:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91a7e:	b9 46 02 00 00       	mov    $0x246,%ecx
   91a83:	e9 31 03 00 00       	jmp    91db9 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6969>
   91a88:	48 8d 35 e3 2c f8 ff 	lea    -0x7d31d(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91a8f:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91a94:	b9 3b 02 00 00       	mov    $0x23b,%ecx
   91a99:	e9 1b 03 00 00       	jmp    91db9 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6969>
   91a9e:	48 8d 35 cd 2c f8 ff 	lea    -0x7d333(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91aa5:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91aaa:	b9 3f 02 00 00       	mov    $0x23f,%ecx
   91aaf:	e9 05 03 00 00       	jmp    91db9 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6969>
   91ab4:	48 8d 35 b7 2c f8 ff 	lea    -0x7d349(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91abb:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91ac0:	b9 34 02 00 00       	mov    $0x234,%ecx
   91ac5:	e9 5c f8 ff ff       	jmp    91326 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5ed6>
   91aca:	48 8d 35 a1 2c f8 ff 	lea    -0x7d35f(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91ad1:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91ad6:	b9 4c 02 00 00       	mov    $0x24c,%ecx
   91adb:	41 b8 6f 00 00 00    	mov    $0x6f,%r8d
   91ae1:	bf 03 00 00 00       	mov    $0x3,%edi
   91ae6:	e8 35 59 08 00       	call   117420 <__wolf_rt_trap_at>
   91aeb:	48 8d 35 80 2c f8 ff 	lea    -0x7d380(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91af2:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91af7:	b9 4d 02 00 00       	mov    $0x24d,%ecx
   91afc:	e9 25 f8 ff ff       	jmp    91326 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5ed6>
   91b01:	48 8d 35 6a 2c f8 ff 	lea    -0x7d396(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91b08:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91b0d:	b9 51 02 00 00       	mov    $0x251,%ecx
   91b12:	e9 0f f8 ff ff       	jmp    91326 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5ed6>
   91b17:	48 8d 35 54 2c f8 ff 	lea    -0x7d3ac(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91b1e:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91b23:	b9 50 02 00 00       	mov    $0x250,%ecx
   91b28:	e9 79 01 00 00       	jmp    91ca6 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6856>
   91b2d:	48 8d 35 3e 2c f8 ff 	lea    -0x7d3c2(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91b34:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91b39:	b9 47 02 00 00       	mov    $0x247,%ecx
   91b3e:	e9 e3 f7 ff ff       	jmp    91326 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5ed6>
   91b43:	48 8d 35 28 2c f8 ff 	lea    -0x7d3d8(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91b4a:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91b4f:	b9 44 02 00 00       	mov    $0x244,%ecx
   91b54:	e9 6d fb ff ff       	jmp    916c6 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6276>
   91b59:	48 8d 35 12 2c f8 ff 	lea    -0x7d3ee(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91b60:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91b65:	b9 42 02 00 00       	mov    $0x242,%ecx
   91b6a:	41 b8 72 00 00 00    	mov    $0x72,%r8d
   91b70:	bf 03 00 00 00       	mov    $0x3,%edi
   91b75:	e8 a6 58 08 00       	call   117420 <__wolf_rt_trap_at>
   91b7a:	48 8d 35 f1 2b f8 ff 	lea    -0x7d40f(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91b81:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91b86:	b9 44 02 00 00       	mov    $0x244,%ecx
   91b8b:	41 b8 7e 00 00 00    	mov    $0x7e,%r8d
   91b91:	bf 03 00 00 00       	mov    $0x3,%edi
   91b96:	e8 85 58 08 00       	call   117420 <__wolf_rt_trap_at>
   91b9b:	48 8d 35 d0 2b f8 ff 	lea    -0x7d430(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91ba2:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91ba7:	b9 37 02 00 00       	mov    $0x237,%ecx
   91bac:	41 b8 61 00 00 00    	mov    $0x61,%r8d
   91bb2:	bf 03 00 00 00       	mov    $0x3,%edi
   91bb7:	e8 64 58 08 00       	call   117420 <__wolf_rt_trap_at>
   91bbc:	48 8d 35 af 2b f8 ff 	lea    -0x7d451(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91bc3:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91bc8:	b9 61 02 00 00       	mov    $0x261,%ecx
   91bcd:	e9 d4 00 00 00       	jmp    91ca6 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6856>
   91bd2:	48 8d 35 99 2b f8 ff 	lea    -0x7d467(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91bd9:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91bde:	b9 35 02 00 00       	mov    $0x235,%ecx
   91be3:	eb 32                	jmp    91c17 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x67c7>
   91be5:	48 8d 35 86 2b f8 ff 	lea    -0x7d47a(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91bec:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91bf1:	b9 3c 02 00 00       	mov    $0x23c,%ecx
   91bf6:	41 b8 7f 00 00 00    	mov    $0x7f,%r8d
   91bfc:	bf 03 00 00 00       	mov    $0x3,%edi
   91c01:	e8 1a 58 08 00       	call   117420 <__wolf_rt_trap_at>
   91c06:	48 8d 35 65 2b f8 ff 	lea    -0x7d49b(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91c0d:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91c12:	b9 48 02 00 00       	mov    $0x248,%ecx
   91c17:	41 b8 59 00 00 00    	mov    $0x59,%r8d
   91c1d:	bf 03 00 00 00       	mov    $0x3,%edi
   91c22:	e8 f9 57 08 00       	call   117420 <__wolf_rt_trap_at>
   91c27:	48 8d 35 44 2b f8 ff 	lea    -0x7d4bc(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91c2e:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91c33:	b9 38 02 00 00       	mov    $0x238,%ecx
   91c38:	41 b8 66 00 00 00    	mov    $0x66,%r8d
   91c3e:	bf 03 00 00 00       	mov    $0x3,%edi
   91c43:	e8 d8 57 08 00       	call   117420 <__wolf_rt_trap_at>
   91c48:	48 8d 35 23 2b f8 ff 	lea    -0x7d4dd(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91c4f:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91c54:	b9 39 02 00 00       	mov    $0x239,%ecx
   91c59:	41 b8 67 00 00 00    	mov    $0x67,%r8d
   91c5f:	bf 03 00 00 00       	mov    $0x3,%edi
   91c64:	e8 b7 57 08 00       	call   117420 <__wolf_rt_trap_at>
   91c69:	48 8d 35 02 2b f8 ff 	lea    -0x7d4fe(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91c70:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91c75:	b9 50 02 00 00       	mov    $0x250,%ecx
   91c7a:	e9 3a 01 00 00       	jmp    91db9 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6969>
   91c7f:	48 8d 35 ec 2a f8 ff 	lea    -0x7d514(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91c86:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91c8b:	b9 c4 02 00 00       	mov    $0x2c4,%ecx
   91c90:	e9 f2 f8 ff ff       	jmp    91587 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6137>
   91c95:	48 8d 35 d6 2a f8 ff 	lea    -0x7d52a(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91c9c:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91ca1:	b9 66 02 00 00       	mov    $0x266,%ecx
   91ca6:	41 b8 55 00 00 00    	mov    $0x55,%r8d
   91cac:	bf 03 00 00 00       	mov    $0x3,%edi
   91cb1:	e8 6a 57 08 00       	call   117420 <__wolf_rt_trap_at>
   91cb6:	48 8d 35 b5 2a f8 ff 	lea    -0x7d54b(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91cbd:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91cc2:	b9 b4 02 00 00       	mov    $0x2b4,%ecx
   91cc7:	41 b8 76 00 00 00    	mov    $0x76,%r8d
   91ccd:	bf 01 00 00 00       	mov    $0x1,%edi
   91cd2:	e8 49 57 08 00       	call   117420 <__wolf_rt_trap_at>
   91cd7:	48 8d 35 94 2a f8 ff 	lea    -0x7d56c(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91cde:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91ce3:	b9 5c 02 00 00       	mov    $0x25c,%ecx
   91ce8:	41 b8 73 00 00 00    	mov    $0x73,%r8d
   91cee:	bf 03 00 00 00       	mov    $0x3,%edi
   91cf3:	e8 28 57 08 00       	call   117420 <__wolf_rt_trap_at>
   91cf8:	48 8d 35 73 2a f8 ff 	lea    -0x7d58d(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91cff:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91d04:	b9 61 02 00 00       	mov    $0x261,%ecx
   91d09:	e9 ab 00 00 00       	jmp    91db9 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6969>
   91d0e:	48 8d 35 5d 2a f8 ff 	lea    -0x7d5a3(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91d15:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91d1a:	b9 bd 02 00 00       	mov    $0x2bd,%ecx
   91d1f:	e9 a5 f8 ff ff       	jmp    915c9 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6179>
   91d24:	48 8d 35 47 2a f8 ff 	lea    -0x7d5b9(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91d2b:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91d30:	b9 d2 02 00 00       	mov    $0x2d2,%ecx
   91d35:	e9 8f f8 ff ff       	jmp    915c9 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6179>
   91d3a:	48 8d 35 31 2a f8 ff 	lea    -0x7d5cf(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91d41:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91d46:	b9 dc 02 00 00       	mov    $0x2dc,%ecx
   91d4b:	e9 79 f8 ff ff       	jmp    915c9 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6179>
   91d50:	48 8d 35 1b 2a f8 ff 	lea    -0x7d5e5(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91d57:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91d5c:	b9 d8 02 00 00       	mov    $0x2d8,%ecx
   91d61:	e9 21 f8 ff ff       	jmp    91587 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6137>
   91d66:	48 8d 35 05 2a f8 ff 	lea    -0x7d5fb(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91d6d:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91d72:	b9 58 02 00 00       	mov    $0x258,%ecx
   91d77:	41 b8 1d 00 00 00    	mov    $0x1d,%r8d
   91d7d:	bf 03 00 00 00       	mov    $0x3,%edi
   91d82:	e8 99 56 08 00       	call   117420 <__wolf_rt_trap_at>
   91d87:	48 8d 35 e4 29 f8 ff 	lea    -0x7d61c(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91d8e:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91d93:	b9 5d 02 00 00       	mov    $0x25d,%ecx
   91d98:	41 b8 4b 00 00 00    	mov    $0x4b,%r8d
   91d9e:	bf 03 00 00 00       	mov    $0x3,%edi
   91da3:	e8 78 56 08 00       	call   117420 <__wolf_rt_trap_at>
   91da8:	48 8d 35 c3 29 f8 ff 	lea    -0x7d63d(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91daf:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91db4:	b9 66 02 00 00       	mov    $0x266,%ecx
   91db9:	41 b8 1f 00 00 00    	mov    $0x1f,%r8d
   91dbf:	bf 03 00 00 00       	mov    $0x3,%edi
   91dc4:	e8 57 56 08 00       	call   117420 <__wolf_rt_trap_at>
   91dc9:	48 8d 35 a2 29 f8 ff 	lea    -0x7d65e(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91dd0:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91dd5:	b9 67 02 00 00       	mov    $0x267,%ecx
   91dda:	e9 47 f5 ff ff       	jmp    91326 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5ed6>
   91ddf:	48 8d 35 8c 29 f8 ff 	lea    -0x7d674(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91de6:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91deb:	b9 5a 02 00 00       	mov    $0x25a,%ecx
   91df0:	41 b8 5d 00 00 00    	mov    $0x5d,%r8d
   91df6:	bf 03 00 00 00       	mov    $0x3,%edi
   91dfb:	e8 20 56 08 00       	call   117420 <__wolf_rt_trap_at>
   91e00:	48 8d 35 6b 29 f8 ff 	lea    -0x7d695(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91e07:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91e0c:	b9 ef 02 00 00       	mov    $0x2ef,%ecx
   91e11:	e9 45 01 00 00       	jmp    91f5b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6b0b>
   91e16:	48 8d 35 55 29 f8 ff 	lea    -0x7d6ab(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91e1d:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91e22:	b9 ee 02 00 00       	mov    $0x2ee,%ecx
   91e27:	e9 e0 f6 ff ff       	jmp    9150c <_Wfl.fl_vm_exec$a5081fe92876eb37+0x60bc>
   91e2c:	48 8d 35 3f 29 f8 ff 	lea    -0x7d6c1(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91e33:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91e38:	b9 5d 02 00 00       	mov    $0x25d,%ecx
   91e3d:	41 b8 27 00 00 00    	mov    $0x27,%r8d
   91e43:	bf 03 00 00 00       	mov    $0x3,%edi
   91e48:	e8 d3 55 08 00       	call   117420 <__wolf_rt_trap_at>
   91e4d:	48 8d 35 1e 29 f8 ff 	lea    -0x7d6e2(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91e54:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91e59:	b9 ea 02 00 00       	mov    $0x2ea,%ecx
   91e5e:	41 b8 25 00 00 00    	mov    $0x25,%r8d
   91e64:	bf 01 00 00 00       	mov    $0x1,%edi
   91e69:	e8 b2 55 08 00       	call   117420 <__wolf_rt_trap_at>
   91e6e:	48 8d 35 fd 28 f8 ff 	lea    -0x7d703(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91e75:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91e7a:	b9 86 02 00 00       	mov    $0x286,%ecx
   91e7f:	e9 57 fc ff ff       	jmp    91adb <_Wfl.fl_vm_exec$a5081fe92876eb37+0x668b>
   91e84:	48 8d 35 e7 28 f8 ff 	lea    -0x7d719(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91e8b:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91e90:	b9 85 02 00 00       	mov    $0x285,%ecx
   91e95:	e9 41 fc ff ff       	jmp    91adb <_Wfl.fl_vm_exec$a5081fe92876eb37+0x668b>
   91e9a:	48 8d 35 d1 28 f8 ff 	lea    -0x7d72f(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91ea1:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91ea6:	b9 ed 02 00 00       	mov    $0x2ed,%ecx
   91eab:	41 b8 71 00 00 00    	mov    $0x71,%r8d
   91eb1:	bf 01 00 00 00       	mov    $0x1,%edi
   91eb6:	e8 65 55 08 00       	call   117420 <__wolf_rt_trap_at>
   91ebb:	48 8d 35 b0 28 f8 ff 	lea    -0x7d750(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91ec2:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91ec7:	b9 1e 03 00 00       	mov    $0x31e,%ecx
   91ecc:	e9 8a 00 00 00       	jmp    91f5b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6b0b>
   91ed1:	48 8d 35 9a 28 f8 ff 	lea    -0x7d766(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91ed8:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91edd:	b9 1b 03 00 00       	mov    $0x31b,%ecx
   91ee2:	e9 e2 f6 ff ff       	jmp    915c9 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6179>
   91ee7:	48 8d 35 84 28 f8 ff 	lea    -0x7d77c(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91eee:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91ef3:	b9 1d 03 00 00       	mov    $0x31d,%ecx
   91ef8:	41 b8 75 00 00 00    	mov    $0x75,%r8d
   91efe:	bf 01 00 00 00       	mov    $0x1,%edi
   91f03:	e8 18 55 08 00       	call   117420 <__wolf_rt_trap_at>
   91f08:	48 8d 35 63 28 f8 ff 	lea    -0x7d79d(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91f0f:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91f14:	b9 94 02 00 00       	mov    $0x294,%ecx
   91f19:	e9 90 08 00 00       	jmp    927ae <_Wfl.fl_vm_exec$a5081fe92876eb37+0x735e>
   91f1e:	48 8d 35 4d 28 f8 ff 	lea    -0x7d7b3(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91f25:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91f2a:	b9 8b 02 00 00       	mov    $0x28b,%ecx
   91f2f:	e9 f2 f3 ff ff       	jmp    91326 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5ed6>
   91f34:	48 8d 35 37 28 f8 ff 	lea    -0x7d7c9(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91f3b:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91f40:	b9 9c 02 00 00       	mov    $0x29c,%ecx
   91f45:	e9 5c fd ff ff       	jmp    91ca6 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6856>
   91f4a:	48 8d 35 21 28 f8 ff 	lea    -0x7d7df(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91f51:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91f56:	b9 7d 02 00 00       	mov    $0x27d,%ecx
   91f5b:	41 b8 6b 00 00 00    	mov    $0x6b,%r8d
   91f61:	bf 01 00 00 00       	mov    $0x1,%edi
   91f66:	e8 b5 54 08 00       	call   117420 <__wolf_rt_trap_at>
   91f6b:	48 8d 35 00 28 f8 ff 	lea    -0x7d800(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91f72:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91f77:	b9 c6 02 00 00       	mov    $0x2c6,%ecx
   91f7c:	e9 6f fe ff ff       	jmp    91df0 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x69a0>
   91f81:	48 8d 35 ea 27 f8 ff 	lea    -0x7d816(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91f88:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91f8d:	b9 88 02 00 00       	mov    $0x288,%ecx
   91f92:	41 b8 69 00 00 00    	mov    $0x69,%r8d
   91f98:	bf 03 00 00 00       	mov    $0x3,%edi
   91f9d:	e8 7e 54 08 00       	call   117420 <__wolf_rt_trap_at>
   91fa2:	48 8d 35 c9 27 f8 ff 	lea    -0x7d837(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91fa9:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91fae:	b9 97 02 00 00       	mov    $0x297,%ecx
   91fb3:	e9 23 fb ff ff       	jmp    91adb <_Wfl.fl_vm_exec$a5081fe92876eb37+0x668b>
   91fb8:	48 8d 35 b3 27 f8 ff 	lea    -0x7d84d(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91fbf:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91fc4:	b9 96 02 00 00       	mov    $0x296,%ecx
   91fc9:	e9 0d fb ff ff       	jmp    91adb <_Wfl.fl_vm_exec$a5081fe92876eb37+0x668b>
   91fce:	48 8d 35 9d 27 f8 ff 	lea    -0x7d863(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91fd5:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91fda:	b9 c6 02 00 00       	mov    $0x2c6,%ecx
   91fdf:	e9 59 fe ff ff       	jmp    91e3d <_Wfl.fl_vm_exec$a5081fe92876eb37+0x69ed>
   91fe4:	48 8d 35 87 27 f8 ff 	lea    -0x7d879(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   91feb:	ba 2d 00 00 00       	mov    $0x2d,%edx
   91ff0:	b9 5c 03 00 00       	mov    $0x35c,%ecx
   91ff5:	e9 cf f5 ff ff       	jmp    915c9 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6179>
   91ffa:	48 8d 35 71 27 f8 ff 	lea    -0x7d88f(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   92001:	ba 2d 00 00 00       	mov    $0x2d,%edx
   92006:	b9 5d 03 00 00       	mov    $0x35d,%ecx
   9200b:	e9 77 f5 ff ff       	jmp    91587 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6137>
   92010:	48 8d 35 5b 27 f8 ff 	lea    -0x7d8a5(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   92017:	ba 2d 00 00 00       	mov    $0x2d,%edx
   9201c:	b9 5b 03 00 00       	mov    $0x35b,%ecx
   92021:	e9 af f4 ff ff       	jmp    914d5 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6085>
   92026:	48 8d 35 45 27 f8 ff 	lea    -0x7d8bb(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   9202d:	ba 2d 00 00 00       	mov    $0x2d,%edx
   92032:	b9 c7 02 00 00       	mov    $0x2c7,%ecx
   92037:	41 b8 5a 00 00 00    	mov    $0x5a,%r8d
   9203d:	bf 03 00 00 00       	mov    $0x3,%edi
   92042:	e8 d9 53 08 00       	call   117420 <__wolf_rt_trap_at>
   92047:	48 8d 35 24 27 f8 ff 	lea    -0x7d8dc(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   9204e:	ba 2d 00 00 00       	mov    $0x2d,%edx
   92053:	b9 6a 02 00 00       	mov    $0x26a,%ecx
   92058:	e9 4a f9 ff ff       	jmp    919a7 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6557>
   9205d:	48 8d 35 0e 27 f8 ff 	lea    -0x7d8f2(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   92064:	ba 2d 00 00 00       	mov    $0x2d,%edx
   92069:	b9 cd 02 00 00       	mov    $0x2cd,%ecx
   9206e:	e9 3b 07 00 00       	jmp    927ae <_Wfl.fl_vm_exec$a5081fe92876eb37+0x735e>
   92073:	48 8d 35 f8 26 f8 ff 	lea    -0x7d908(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   9207a:	ba 2d 00 00 00       	mov    $0x2d,%edx
   9207f:	b9 d5 02 00 00       	mov    $0x2d5,%ecx
   92084:	e9 25 07 00 00       	jmp    927ae <_Wfl.fl_vm_exec$a5081fe92876eb37+0x735e>
   92089:	48 8d 35 e2 26 f8 ff 	lea    -0x7d91e(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   92090:	ba 2d 00 00 00       	mov    $0x2d,%edx
   92095:	b9 77 02 00 00       	mov    $0x277,%ecx
   9209a:	e9 4d f8 ff ff       	jmp    918ec <_Wfl.fl_vm_exec$a5081fe92876eb37+0x649c>
   9209f:	48 8d 35 cc 26 f8 ff 	lea    -0x7d934(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   920a6:	ba 2d 00 00 00       	mov    $0x2d,%edx
   920ab:	b9 9c 02 00 00       	mov    $0x29c,%ecx
   920b0:	e9 04 fd ff ff       	jmp    91db9 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6969>
   920b5:	48 8d 35 b6 26 f8 ff 	lea    -0x7d94a(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   920bc:	ba 2d 00 00 00       	mov    $0x2d,%edx
   920c1:	b9 cb 02 00 00       	mov    $0x2cb,%ecx
   920c6:	e9 ac fc ff ff       	jmp    91d77 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6927>
   920cb:	48 8d 35 a0 26 f8 ff 	lea    -0x7d960(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   920d2:	ba 2d 00 00 00       	mov    $0x2d,%edx
   920d7:	b9 a6 02 00 00       	mov    $0x2a6,%ecx
   920dc:	e9 c5 fb ff ff       	jmp    91ca6 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6856>
   920e1:	48 8d 35 8a 26 f8 ff 	lea    -0x7d976(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   920e8:	ba 2d 00 00 00       	mov    $0x2d,%edx
   920ed:	b9 e0 02 00 00       	mov    $0x2e0,%ecx
   920f2:	e9 b7 06 00 00       	jmp    927ae <_Wfl.fl_vm_exec$a5081fe92876eb37+0x735e>
   920f7:	48 8d 35 74 26 f8 ff 	lea    -0x7d98c(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   920fe:	ba 2d 00 00 00       	mov    $0x2d,%edx
   92103:	b9 78 03 00 00       	mov    $0x378,%ecx
   92108:	e9 bc f4 ff ff       	jmp    915c9 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6179>
   9210d:	48 8d 35 5e 26 f8 ff 	lea    -0x7d9a2(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   92114:	ba 2d 00 00 00       	mov    $0x2d,%edx
   92119:	b9 76 03 00 00       	mov    $0x376,%ecx
   9211e:	41 b8 68 00 00 00    	mov    $0x68,%r8d
   92124:	bf 01 00 00 00       	mov    $0x1,%edi
   92129:	e8 f2 52 08 00       	call   117420 <__wolf_rt_trap_at>
   9212e:	48 8d 35 3d 26 f8 ff 	lea    -0x7d9c3(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   92135:	ba 2d 00 00 00       	mov    $0x2d,%edx
   9213a:	b9 74 03 00 00       	mov    $0x374,%ecx
   9213f:	e9 43 f4 ff ff       	jmp    91587 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6137>
   92144:	48 8d 35 27 26 f8 ff 	lea    -0x7d9d9(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   9214b:	ba 2d 00 00 00       	mov    $0x2d,%edx
   92150:	b9 a3 02 00 00       	mov    $0x2a3,%ecx
   92155:	e9 1d fc ff ff       	jmp    91d77 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6927>
   9215a:	48 8d 35 11 26 f8 ff 	lea    -0x7d9ef(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   92161:	ba 2d 00 00 00       	mov    $0x2d,%edx
   92166:	b9 93 03 00 00       	mov    $0x393,%ecx
   9216b:	e9 65 f3 ff ff       	jmp    914d5 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6085>
   92170:	48 8d 35 fb 25 f8 ff 	lea    -0x7da05(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   92177:	ba 2d 00 00 00       	mov    $0x2d,%edx
   9217c:	b9 6e 03 00 00       	mov    $0x36e,%ecx
   92181:	e9 01 f4 ff ff       	jmp    91587 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6137>
   92186:	48 8d 35 e5 25 f8 ff 	lea    -0x7da1b(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   9218d:	ba 2d 00 00 00       	mov    $0x2d,%edx
   92192:	b9 91 03 00 00       	mov    $0x391,%ecx
   92197:	e9 0b f8 ff ff       	jmp    919a7 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6557>
   9219c:	48 8d 35 cf 25 f8 ff 	lea    -0x7da31(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   921a3:	ba 2d 00 00 00       	mov    $0x2d,%edx
   921a8:	b9 51 03 00 00       	mov    $0x351,%ecx
   921ad:	e9 d5 f3 ff ff       	jmp    91587 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6137>
   921b2:	48 8d 35 b9 25 f8 ff 	lea    -0x7da47(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   921b9:	ba 2d 00 00 00       	mov    $0x2d,%edx
   921be:	b9 64 03 00 00       	mov    $0x364,%ecx
   921c3:	e9 01 f4 ff ff       	jmp    915c9 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6179>
   921c8:	48 8d 35 a3 25 f8 ff 	lea    -0x7da5d(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   921cf:	ba 2d 00 00 00       	mov    $0x2d,%edx
   921d4:	b9 8c 03 00 00       	mov    $0x38c,%ecx
   921d9:	e9 eb f3 ff ff       	jmp    915c9 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6179>
   921de:	48 8d 35 8d 25 f8 ff 	lea    -0x7da73(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   921e5:	ba 2d 00 00 00       	mov    $0x2d,%edx
   921ea:	b9 8d 03 00 00       	mov    $0x38d,%ecx
   921ef:	e9 18 f3 ff ff       	jmp    9150c <_Wfl.fl_vm_exec$a5081fe92876eb37+0x60bc>
   921f4:	48 8d 35 77 25 f8 ff 	lea    -0x7da89(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   921fb:	ba 2d 00 00 00       	mov    $0x2d,%edx
   92200:	b9 65 03 00 00       	mov    $0x365,%ecx
   92205:	e9 cb f2 ff ff       	jmp    914d5 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6085>
   9220a:	48 8d 35 61 25 f8 ff 	lea    -0x7da9f(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   92211:	ba 2d 00 00 00       	mov    $0x2d,%edx
   92216:	b9 8e 03 00 00       	mov    $0x38e,%ecx
   9221b:	e9 67 f3 ff ff       	jmp    91587 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6137>
   92220:	48 8d 35 4b 25 f8 ff 	lea    -0x7dab5(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   92227:	ba 2d 00 00 00       	mov    $0x2d,%edx
   9222c:	b9 60 03 00 00       	mov    $0x360,%ecx
   92231:	e9 93 f3 ff ff       	jmp    915c9 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6179>
   92236:	48 8d 35 35 25 f8 ff 	lea    -0x7dacb(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   9223d:	ba 2d 00 00 00       	mov    $0x2d,%edx
   92242:	b9 df 02 00 00       	mov    $0x2df,%ecx
   92247:	e9 62 05 00 00       	jmp    927ae <_Wfl.fl_vm_exec$a5081fe92876eb37+0x735e>
   9224c:	48 8d 35 1f 25 f8 ff 	lea    -0x7dae1(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   92253:	ba 2d 00 00 00       	mov    $0x2d,%edx
   92258:	b9 b4 02 00 00       	mov    $0x2b4,%ecx
   9225d:	41 b8 82 00 00 00    	mov    $0x82,%r8d
   92263:	bf 03 00 00 00       	mov    $0x3,%edi
   92268:	e8 b3 51 08 00       	call   117420 <__wolf_rt_trap_at>
   9226d:	48 8d 35 fe 24 f8 ff 	lea    -0x7db02(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   92274:	ba 2d 00 00 00       	mov    $0x2d,%edx
   92279:	b9 60 03 00 00       	mov    $0x360,%ecx
   9227e:	e9 23 fa ff ff       	jmp    91ca6 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6856>
   92283:	48 8d 35 e8 24 f8 ff 	lea    -0x7db18(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   9228a:	ba 2d 00 00 00       	mov    $0x2d,%edx
   9228f:	b9 78 03 00 00       	mov    $0x378,%ecx
   92294:	e9 20 fb ff ff       	jmp    91db9 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6969>
   92299:	48 8d 35 d2 24 f8 ff 	lea    -0x7db2e(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   922a0:	ba 2d 00 00 00       	mov    $0x2d,%edx
   922a5:	b9 77 02 00 00       	mov    $0x277,%ecx
   922aa:	e9 c8 fa ff ff       	jmp    91d77 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6927>
   922af:	48 8d 35 bc 24 f8 ff 	lea    -0x7db44(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   922b6:	ba 2d 00 00 00       	mov    $0x2d,%edx
   922bb:	b9 b5 02 00 00       	mov    $0x2b5,%ecx
   922c0:	e9 61 f0 ff ff       	jmp    91326 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5ed6>
   922c5:	48 8d 35 a6 24 f8 ff 	lea    -0x7db5a(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   922cc:	ba 2d 00 00 00       	mov    $0x2d,%edx
   922d1:	b9 bf 02 00 00       	mov    $0x2bf,%ecx
   922d6:	e9 4b f0 ff ff       	jmp    91326 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5ed6>
   922db:	48 8d 35 90 24 f8 ff 	lea    -0x7db70(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   922e2:	ba 2d 00 00 00       	mov    $0x2d,%edx
   922e7:	b9 1d 03 00 00       	mov    $0x31d,%ecx
   922ec:	41 b8 81 00 00 00    	mov    $0x81,%r8d
   922f2:	bf 03 00 00 00       	mov    $0x3,%edi
   922f7:	e8 24 51 08 00       	call   117420 <__wolf_rt_trap_at>
   922fc:	48 8d 35 6f 24 f8 ff 	lea    -0x7db91(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   92303:	ba 2d 00 00 00       	mov    $0x2d,%edx
   92308:	b9 5b 03 00 00       	mov    $0x35b,%ecx
   9230d:	e9 50 f7 ff ff       	jmp    91a62 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6612>
   92312:	48 8d 35 59 24 f8 ff 	lea    -0x7dba7(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   92319:	ba 2d 00 00 00       	mov    $0x2d,%edx
   9231e:	b9 78 03 00 00       	mov    $0x378,%ecx
   92323:	e9 7e f9 ff ff       	jmp    91ca6 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6856>
   92328:	48 8d 35 43 24 f8 ff 	lea    -0x7dbbd(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   9232f:	ba 2d 00 00 00       	mov    $0x2d,%edx
   92334:	b9 7d 02 00 00       	mov    $0x27d,%ecx
   92339:	e9 5f 06 00 00       	jmp    9299d <_Wfl.fl_vm_exec$a5081fe92876eb37+0x754d>
   9233e:	48 8d 35 2d 24 f8 ff 	lea    -0x7dbd3(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   92345:	ba 2d 00 00 00       	mov    $0x2d,%edx
   9234a:	b9 99 02 00 00       	mov    $0x299,%ecx
   9234f:	e9 3e fc ff ff       	jmp    91f92 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6b42>
   92354:	48 8d 35 17 24 f8 ff 	lea    -0x7dbe9(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   9235b:	ba 2d 00 00 00       	mov    $0x2d,%edx
   92360:	b9 dc 02 00 00       	mov    $0x2dc,%ecx
   92365:	e9 4f fa ff ff       	jmp    91db9 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6969>
   9236a:	48 8d 35 01 24 f8 ff 	lea    -0x7dbff(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   92371:	ba 2d 00 00 00       	mov    $0x2d,%edx
   92376:	b9 bd 02 00 00       	mov    $0x2bd,%ecx
   9237b:	e9 26 f9 ff ff       	jmp    91ca6 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6856>
   92380:	48 8d 35 eb 23 f8 ff 	lea    -0x7dc15(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   92387:	ba 2d 00 00 00       	mov    $0x2d,%edx
   9238c:	b9 ab 02 00 00       	mov    $0x2ab,%ecx
   92391:	41 b8 84 00 00 00    	mov    $0x84,%r8d
   92397:	bf 03 00 00 00       	mov    $0x3,%edi
   9239c:	e8 7f 50 08 00       	call   117420 <__wolf_rt_trap_at>
   923a1:	48 8d 35 ca 23 f8 ff 	lea    -0x7dc36(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   923a8:	ba 2d 00 00 00       	mov    $0x2d,%edx
   923ad:	b9 1c 03 00 00       	mov    $0x31c,%ecx
   923b2:	e9 6f ef ff ff       	jmp    91326 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5ed6>
   923b7:	48 8d 35 b4 23 f8 ff 	lea    -0x7dc4c(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   923be:	ba 2d 00 00 00       	mov    $0x2d,%edx
   923c3:	b9 60 03 00 00       	mov    $0x360,%ecx
   923c8:	e9 ec f9 ff ff       	jmp    91db9 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6969>
   923cd:	48 8d 35 9e 23 f8 ff 	lea    -0x7dc62(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   923d4:	ba 2d 00 00 00       	mov    $0x2d,%edx
   923d9:	b9 1b 03 00 00       	mov    $0x31b,%ecx
   923de:	e9 c3 f8 ff ff       	jmp    91ca6 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6856>
   923e3:	48 8d 35 88 23 f8 ff 	lea    -0x7dc78(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   923ea:	ba 2d 00 00 00       	mov    $0x2d,%edx
   923ef:	b9 2b 03 00 00       	mov    $0x32b,%ecx
   923f4:	e9 36 05 00 00       	jmp    9292f <_Wfl.fl_vm_exec$a5081fe92876eb37+0x74df>
   923f9:	48 8d 35 72 23 f8 ff 	lea    -0x7dc8e(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   92400:	ba 2d 00 00 00       	mov    $0x2d,%edx
   92405:	b9 dc 02 00 00       	mov    $0x2dc,%ecx
   9240a:	e9 97 f8 ff ff       	jmp    91ca6 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6856>
   9240f:	48 8d 35 5c 23 f8 ff 	lea    -0x7dca4(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   92416:	ba 2d 00 00 00       	mov    $0x2d,%edx
   9241b:	b9 be 02 00 00       	mov    $0x2be,%ecx
   92420:	e9 01 ef ff ff       	jmp    91326 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5ed6>
   92425:	48 8d 35 46 23 f8 ff 	lea    -0x7dcba(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   9242c:	ba 2d 00 00 00       	mov    $0x2d,%edx
   92431:	b9 f1 02 00 00       	mov    $0x2f1,%ecx
   92436:	e9 f4 04 00 00       	jmp    9292f <_Wfl.fl_vm_exec$a5081fe92876eb37+0x74df>
   9243b:	48 8d 35 30 23 f8 ff 	lea    -0x7dcd0(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   92442:	ba 2d 00 00 00       	mov    $0x2d,%edx
   92447:	b9 c1 02 00 00       	mov    $0x2c1,%ecx
   9244c:	e9 bc f4 ff ff       	jmp    9190d <_Wfl.fl_vm_exec$a5081fe92876eb37+0x64bd>
   92451:	48 8d 35 1a 23 f8 ff 	lea    -0x7dce6(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   92458:	ba 2d 00 00 00       	mov    $0x2d,%edx
   9245d:	b9 1b 03 00 00       	mov    $0x31b,%ecx
   92462:	e9 52 f9 ff ff       	jmp    91db9 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6969>
   92467:	48 8d 35 04 23 f8 ff 	lea    -0x7dcfc(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   9246e:	ba 2d 00 00 00       	mov    $0x2d,%edx
   92473:	b9 42 03 00 00       	mov    $0x342,%ecx
   92478:	41 b8 21 00 00 00    	mov    $0x21,%r8d
   9247e:	bf 04 00 00 00       	mov    $0x4,%edi
   92483:	e8 98 4f 08 00       	call   117420 <__wolf_rt_trap_at>
   92488:	48 8d 35 e3 22 f8 ff 	lea    -0x7dd1d(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   9248f:	ba 2d 00 00 00       	mov    $0x2d,%edx
   92494:	b9 bd 02 00 00       	mov    $0x2bd,%ecx
   92499:	e9 1b f9 ff ff       	jmp    91db9 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6969>
   9249e:	48 8d 35 cd 22 f8 ff 	lea    -0x7dd33(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   924a5:	ba 2d 00 00 00       	mov    $0x2d,%edx
   924aa:	b9 68 03 00 00       	mov    $0x368,%ecx
   924af:	41 b8 6e 00 00 00    	mov    $0x6e,%r8d
   924b5:	bf 03 00 00 00       	mov    $0x3,%edi
   924ba:	e8 61 4f 08 00       	call   117420 <__wolf_rt_trap_at>
   924bf:	48 8d 35 ac 22 f8 ff 	lea    -0x7dd54(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   924c6:	ba 2d 00 00 00       	mov    $0x2d,%edx
   924cb:	b9 a7 02 00 00       	mov    $0x2a7,%ecx
   924d0:	e9 51 ee ff ff       	jmp    91326 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5ed6>
   924d5:	48 8d 35 96 22 f8 ff 	lea    -0x7dd6a(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   924dc:	ba 2d 00 00 00       	mov    $0x2d,%edx
   924e1:	b9 bf 02 00 00       	mov    $0x2bf,%ecx
   924e6:	e9 22 f4 ff ff       	jmp    9190d <_Wfl.fl_vm_exec$a5081fe92876eb37+0x64bd>
   924eb:	48 8d 35 80 22 f8 ff 	lea    -0x7dd80(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   924f2:	ba 2d 00 00 00       	mov    $0x2d,%edx
   924f7:	b9 8e 03 00 00       	mov    $0x38e,%ecx
   924fc:	e9 25 ee ff ff       	jmp    91326 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5ed6>
   92501:	48 8d 35 6a 22 f8 ff 	lea    -0x7dd96(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   92508:	ba 2d 00 00 00       	mov    $0x2d,%edx
   9250d:	b9 fc 02 00 00       	mov    $0x2fc,%ecx
   92512:	41 b8 63 00 00 00    	mov    $0x63,%r8d
   92518:	bf 03 00 00 00       	mov    $0x3,%edi
   9251d:	e8 fe 4e 08 00       	call   117420 <__wolf_rt_trap_at>
   92522:	48 8d 35 49 22 f8 ff 	lea    -0x7ddb7(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   92529:	ba 2d 00 00 00       	mov    $0x2d,%edx
   9252e:	b9 8d 03 00 00       	mov    $0x38d,%ecx
   92533:	e9 a3 f5 ff ff       	jmp    91adb <_Wfl.fl_vm_exec$a5081fe92876eb37+0x668b>
   92538:	48 8d 35 33 22 f8 ff 	lea    -0x7ddcd(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   9253f:	ba 2d 00 00 00       	mov    $0x2d,%edx
   92544:	b9 42 03 00 00       	mov    $0x342,%ecx
   92549:	41 b8 21 00 00 00    	mov    $0x21,%r8d
   9254f:	bf 01 00 00 00       	mov    $0x1,%edi
   92554:	e8 c7 4e 08 00       	call   117420 <__wolf_rt_trap_at>
   92559:	48 8d 35 12 22 f8 ff 	lea    -0x7ddee(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   92560:	ba 2d 00 00 00       	mov    $0x2d,%edx
   92565:	b9 5b 03 00 00       	mov    $0x35b,%ecx
   9256a:	e9 bf f3 ff ff       	jmp    9192e <_Wfl.fl_vm_exec$a5081fe92876eb37+0x64de>
   9256f:	48 8d 35 fc 21 f8 ff 	lea    -0x7de04(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   92576:	ba 2d 00 00 00       	mov    $0x2d,%edx
   9257b:	b9 c2 02 00 00       	mov    $0x2c2,%ecx
   92580:	e9 a1 ed ff ff       	jmp    91326 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5ed6>
   92585:	48 8d 35 e6 21 f8 ff 	lea    -0x7de1a(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   9258c:	ba 2d 00 00 00       	mov    $0x2d,%edx
   92591:	b9 7c 03 00 00       	mov    $0x37c,%ecx
   92596:	e9 0c f4 ff ff       	jmp    919a7 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6557>
   9259b:	48 8d 35 d0 21 f8 ff 	lea    -0x7de30(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   925a2:	ba 2d 00 00 00       	mov    $0x2d,%edx
   925a7:	b9 1f 03 00 00       	mov    $0x31f,%ecx
   925ac:	e9 75 ed ff ff       	jmp    91326 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5ed6>
   925b1:	48 8d 35 ba 21 f8 ff 	lea    -0x7de46(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   925b8:	ba 2d 00 00 00       	mov    $0x2d,%edx
   925bd:	b9 9f 02 00 00       	mov    $0x29f,%ecx
   925c2:	e9 21 f7 ff ff       	jmp    91ce8 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6898>
   925c7:	48 8d 35 a4 21 f8 ff 	lea    -0x7de5c(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   925ce:	ba 2d 00 00 00       	mov    $0x2d,%edx
   925d3:	b9 5c 03 00 00       	mov    $0x35c,%ecx
   925d8:	e9 c9 f6 ff ff       	jmp    91ca6 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6856>
   925dd:	48 8d 35 8e 21 f8 ff 	lea    -0x7de72(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   925e4:	ba 2d 00 00 00       	mov    $0x2d,%edx
   925e9:	b9 77 02 00 00       	mov    $0x277,%ecx
   925ee:	e9 19 01 00 00       	jmp    9270c <_Wfl.fl_vm_exec$a5081fe92876eb37+0x72bc>
   925f3:	48 8d 35 78 21 f8 ff 	lea    -0x7de88(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   925fa:	ba 2d 00 00 00       	mov    $0x2d,%edx
   925ff:	b9 ea 02 00 00       	mov    $0x2ea,%ecx
   92604:	41 b8 79 00 00 00    	mov    $0x79,%r8d
   9260a:	bf 03 00 00 00       	mov    $0x3,%edi
   9260f:	e8 0c 4e 08 00       	call   117420 <__wolf_rt_trap_at>
   92614:	48 8d 35 57 21 f8 ff 	lea    -0x7dea9(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   9261b:	ba 2d 00 00 00       	mov    $0x2d,%edx
   92620:	b9 93 03 00 00       	mov    $0x393,%ecx
   92625:	e9 38 f4 ff ff       	jmp    91a62 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6612>
   9262a:	48 8d 35 41 21 f8 ff 	lea    -0x7debf(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   92631:	ba 2d 00 00 00       	mov    $0x2d,%edx
   92636:	b9 76 03 00 00       	mov    $0x376,%ecx
   9263b:	41 b8 74 00 00 00    	mov    $0x74,%r8d
   92641:	bf 03 00 00 00       	mov    $0x3,%edi
   92646:	e8 d5 4d 08 00       	call   117420 <__wolf_rt_trap_at>
   9264b:	48 8d 35 20 21 f8 ff 	lea    -0x7dee0(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   92652:	ba 2d 00 00 00       	mov    $0x2d,%edx
   92657:	b9 a2 02 00 00       	mov    $0x2a2,%ecx
   9265c:	41 b8 2f 00 00 00    	mov    $0x2f,%r8d
   92662:	bf 03 00 00 00       	mov    $0x3,%edi
   92667:	e8 b4 4d 08 00       	call   117420 <__wolf_rt_trap_at>
   9266c:	48 8d 35 ff 20 f8 ff 	lea    -0x7df01(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   92673:	ba 2d 00 00 00       	mov    $0x2d,%edx
   92678:	b9 02 03 00 00       	mov    $0x302,%ecx
   9267d:	e9 f5 f6 ff ff       	jmp    91d77 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6927>
   92682:	48 8d 35 e9 20 f8 ff 	lea    -0x7df17(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   92689:	ba 2d 00 00 00       	mov    $0x2d,%edx
   9268e:	b9 ed 02 00 00       	mov    $0x2ed,%ecx
   92693:	41 b8 7d 00 00 00    	mov    $0x7d,%r8d
   92699:	bf 03 00 00 00       	mov    $0x3,%edi
   9269e:	e8 7d 4d 08 00       	call   117420 <__wolf_rt_trap_at>
   926a3:	48 8d 35 c8 20 f8 ff 	lea    -0x7df38(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   926aa:	ba 2d 00 00 00       	mov    $0x2d,%edx
   926af:	b9 d2 02 00 00       	mov    $0x2d2,%ecx
   926b4:	e9 00 f7 ff ff       	jmp    91db9 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6969>
   926b9:	48 8d 35 b2 20 f8 ff 	lea    -0x7df4e(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   926c0:	ba 2d 00 00 00       	mov    $0x2d,%edx
   926c5:	b9 93 03 00 00       	mov    $0x393,%ecx
   926ca:	e9 5f f2 ff ff       	jmp    9192e <_Wfl.fl_vm_exec$a5081fe92876eb37+0x64de>
   926cf:	48 8d 35 9c 20 f8 ff 	lea    -0x7df64(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   926d6:	ba 2d 00 00 00       	mov    $0x2d,%edx
   926db:	b9 51 03 00 00       	mov    $0x351,%ecx
   926e0:	e9 41 ec ff ff       	jmp    91326 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5ed6>
   926e5:	48 8d 35 86 20 f8 ff 	lea    -0x7df7a(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   926ec:	ba 2d 00 00 00       	mov    $0x2d,%edx
   926f1:	b9 7a 02 00 00       	mov    $0x27a,%ecx
   926f6:	e9 7c f6 ff ff       	jmp    91d77 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6927>
   926fb:	48 8d 35 70 20 f8 ff 	lea    -0x7df90(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   92702:	ba 2d 00 00 00       	mov    $0x2d,%edx
   92707:	b9 02 03 00 00       	mov    $0x302,%ecx
   9270c:	41 b8 1d 00 00 00    	mov    $0x1d,%r8d
   92712:	bf 04 00 00 00       	mov    $0x4,%edi
   92717:	e8 04 4d 08 00       	call   117420 <__wolf_rt_trap_at>
   9271c:	48 8d 35 4f 20 f8 ff 	lea    -0x7dfb1(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   92723:	ba 2d 00 00 00       	mov    $0x2d,%edx
   92728:	b9 6a 02 00 00       	mov    $0x26a,%ecx
   9272d:	eb 7f                	jmp    927ae <_Wfl.fl_vm_exec$a5081fe92876eb37+0x735e>
   9272f:	48 8d 35 3c 20 f8 ff 	lea    -0x7dfc4(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   92736:	ba 2d 00 00 00       	mov    $0x2d,%edx
   9273b:	b9 02 03 00 00       	mov    $0x302,%ecx
   92740:	e9 a7 f1 ff ff       	jmp    918ec <_Wfl.fl_vm_exec$a5081fe92876eb37+0x649c>
   92745:	48 8d 35 26 20 f8 ff 	lea    -0x7dfda(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   9274c:	ba 2d 00 00 00       	mov    $0x2d,%edx
   92751:	b9 d2 02 00 00       	mov    $0x2d2,%ecx
   92756:	e9 4b f5 ff ff       	jmp    91ca6 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6856>
   9275b:	48 8d 35 10 20 f8 ff 	lea    -0x7dff0(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   92762:	ba 2d 00 00 00       	mov    $0x2d,%edx
   92767:	b9 ee 02 00 00       	mov    $0x2ee,%ecx
   9276c:	e9 6a f3 ff ff       	jmp    91adb <_Wfl.fl_vm_exec$a5081fe92876eb37+0x668b>
   92771:	48 8d 35 fa 1f f8 ff 	lea    -0x7e006(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   92778:	ba 2d 00 00 00       	mov    $0x2d,%edx
   9277d:	b9 6a 02 00 00       	mov    $0x26a,%ecx
   92782:	e9 a8 01 00 00       	jmp    9292f <_Wfl.fl_vm_exec$a5081fe92876eb37+0x74df>
   92787:	48 8d 35 e4 1f f8 ff 	lea    -0x7e01c(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   9278e:	ba 2d 00 00 00       	mov    $0x2d,%edx
   92793:	b9 06 03 00 00       	mov    $0x306,%ecx
   92798:	e9 4b f5 ff ff       	jmp    91ce8 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6898>
   9279d:	48 8d 35 ce 1f f8 ff 	lea    -0x7e032(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   927a4:	ba 2d 00 00 00       	mov    $0x2d,%edx
   927a9:	b9 91 03 00 00       	mov    $0x391,%ecx
   927ae:	41 b8 19 00 00 00    	mov    $0x19,%r8d
   927b4:	bf 03 00 00 00       	mov    $0x3,%edi
   927b9:	e8 62 4c 08 00       	call   117420 <__wolf_rt_trap_at>
   927be:	48 8d 35 ad 1f f8 ff 	lea    -0x7e053(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   927c5:	ba 2d 00 00 00       	mov    $0x2d,%edx
   927ca:	b9 64 03 00 00       	mov    $0x364,%ecx
   927cf:	e9 e5 f5 ff ff       	jmp    91db9 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6969>
   927d4:	48 8d 35 97 1f f8 ff 	lea    -0x7e069(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   927db:	ba 2d 00 00 00       	mov    $0x2d,%edx
   927e0:	b9 ef 02 00 00       	mov    $0x2ef,%ecx
   927e5:	e9 b3 01 00 00       	jmp    9299d <_Wfl.fl_vm_exec$a5081fe92876eb37+0x754d>
   927ea:	48 8d 35 81 1f f8 ff 	lea    -0x7e07f(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   927f1:	ba 2d 00 00 00       	mov    $0x2d,%edx
   927f6:	b9 8c 03 00 00       	mov    $0x38c,%ecx
   927fb:	e9 b9 f5 ff ff       	jmp    91db9 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6969>
   92800:	48 8d 35 6b 1f f8 ff 	lea    -0x7e095(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   92807:	ba 2d 00 00 00       	mov    $0x2d,%edx
   9280c:	b9 57 03 00 00       	mov    $0x357,%ecx
   92811:	e9 96 f3 ff ff       	jmp    91bac <_Wfl.fl_vm_exec$a5081fe92876eb37+0x675c>
   92816:	48 8d 35 55 1f f8 ff 	lea    -0x7e0ab(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   9281d:	ba 2d 00 00 00       	mov    $0x2d,%edx
   92822:	b9 73 02 00 00       	mov    $0x273,%ecx
   92827:	41 b8 71 00 00 00    	mov    $0x71,%r8d
   9282d:	bf 03 00 00 00       	mov    $0x3,%edi
   92832:	e8 e9 4b 08 00       	call   117420 <__wolf_rt_trap_at>
   92837:	48 8d 35 34 1f f8 ff 	lea    -0x7e0cc(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   9283e:	ba 2d 00 00 00       	mov    $0x2d,%edx
   92843:	b9 64 03 00 00       	mov    $0x364,%ecx
   92848:	e9 59 f4 ff ff       	jmp    91ca6 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6856>
   9284d:	48 8d 35 1e 1f f8 ff 	lea    -0x7e0e2(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   92854:	ba 2d 00 00 00       	mov    $0x2d,%edx
   92859:	b9 a6 02 00 00       	mov    $0x2a6,%ecx
   9285e:	e9 56 f5 ff ff       	jmp    91db9 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6969>
   92863:	48 8d 35 08 1f f8 ff 	lea    -0x7e0f8(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   9286a:	ba 2d 00 00 00       	mov    $0x2d,%edx
   9286f:	b9 8c 03 00 00       	mov    $0x38c,%ecx
   92874:	e9 2d f4 ff ff       	jmp    91ca6 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6856>
   92879:	48 8d 35 f2 1e f8 ff 	lea    -0x7e10e(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   92880:	ba 2d 00 00 00       	mov    $0x2d,%edx
   92885:	b9 42 03 00 00       	mov    $0x342,%ecx
   9288a:	41 b8 21 00 00 00    	mov    $0x21,%r8d
   92890:	bf 03 00 00 00       	mov    $0x3,%edi
   92895:	e8 86 4b 08 00       	call   117420 <__wolf_rt_trap_at>
   9289a:	48 8d 35 d1 1e f8 ff 	lea    -0x7e12f(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   928a1:	ba 2d 00 00 00       	mov    $0x2d,%edx
   928a6:	b9 cf 02 00 00       	mov    $0x2cf,%ecx
   928ab:	41 b8 5c 00 00 00    	mov    $0x5c,%r8d
   928b1:	bf 03 00 00 00       	mov    $0x3,%edi
   928b6:	e8 65 4b 08 00       	call   117420 <__wolf_rt_trap_at>
   928bb:	48 8d 35 b0 1e f8 ff 	lea    -0x7e150(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   928c2:	ba 2d 00 00 00       	mov    $0x2d,%edx
   928c7:	b9 d9 02 00 00       	mov    $0x2d9,%ecx
   928cc:	e9 46 f3 ff ff       	jmp    91c17 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x67c7>
   928d1:	48 8d 35 9a 1e f8 ff 	lea    -0x7e166(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   928d8:	ba 2d 00 00 00       	mov    $0x2d,%edx
   928dd:	b9 65 03 00 00       	mov    $0x365,%ecx
   928e2:	e9 47 f0 ff ff       	jmp    9192e <_Wfl.fl_vm_exec$a5081fe92876eb37+0x64de>
   928e7:	48 8d 35 84 1e f8 ff 	lea    -0x7e17c(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   928ee:	ba 2d 00 00 00       	mov    $0x2d,%edx
   928f3:	b9 58 03 00 00       	mov    $0x358,%ecx
   928f8:	41 b8 64 00 00 00    	mov    $0x64,%r8d
   928fe:	bf 03 00 00 00       	mov    $0x3,%edi
   92903:	e8 18 4b 08 00       	call   117420 <__wolf_rt_trap_at>
   92908:	48 8d 35 63 1e f8 ff 	lea    -0x7e19d(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   9290f:	ba 2d 00 00 00       	mov    $0x2d,%edx
   92914:	b9 66 03 00 00       	mov    $0x366,%ecx
   92919:	e9 08 ea ff ff       	jmp    91326 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5ed6>
   9291e:	48 8d 35 4d 1e f8 ff 	lea    -0x7e1b3(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   92925:	ba 2d 00 00 00       	mov    $0x2d,%edx
   9292a:	b9 53 03 00 00       	mov    $0x353,%ecx
   9292f:	41 b8 19 00 00 00    	mov    $0x19,%r8d
   92935:	bf 04 00 00 00       	mov    $0x4,%edi
   9293a:	e8 e1 4a 08 00       	call   117420 <__wolf_rt_trap_at>
   9293f:	48 8d 35 2c 1e f8 ff 	lea    -0x7e1d4(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   92946:	ba 2d 00 00 00       	mov    $0x2d,%edx
   9294b:	b9 48 03 00 00       	mov    $0x348,%ecx
   92950:	e9 04 f3 ff ff       	jmp    91c59 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6809>
   92955:	48 8d 35 16 1e f8 ff 	lea    -0x7e1ea(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   9295c:	ba 2d 00 00 00       	mov    $0x2d,%edx
   92961:	b9 ba 02 00 00       	mov    $0x2ba,%ecx
   92966:	41 b8 5e 00 00 00    	mov    $0x5e,%r8d
   9296c:	bf 03 00 00 00       	mov    $0x3,%edi
   92971:	e8 aa 4a 08 00       	call   117420 <__wolf_rt_trap_at>
   92976:	48 8d 35 f5 1d f8 ff 	lea    -0x7e20b(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   9297d:	ba 2d 00 00 00       	mov    $0x2d,%edx
   92982:	b9 28 03 00 00       	mov    $0x328,%ecx
   92987:	e9 5c f3 ff ff       	jmp    91ce8 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x6898>
   9298c:	48 8d 35 df 1d f8 ff 	lea    -0x7e221(%rip),%rsi        # 14772 <__abi_tag+0x14476>
   92993:	ba 2d 00 00 00       	mov    $0x2d,%edx
   92998:	b9 1e 03 00 00       	mov    $0x31e,%ecx
   9299d:	41 b8 77 00 00 00    	mov    $0x77,%r8d
   929a3:	bf 03 00 00 00       	mov    $0x3,%edi
   929a8:	e8 73 4a 08 00       	call   117420 <__wolf_rt_trap_at>
   929ad:	0f 1f 00             	nopl   (%rax)
