0000000000099600 <_Wfl.fl_vm_exec$a5081fe92876eb37>:
   99600:	55                   	push   %rbp
   99601:	41 57                	push   %r15
   99603:	41 56                	push   %r14
   99605:	41 55                	push   %r13
   99607:	41 54                	push   %r12
   99609:	53                   	push   %rbx
   9960a:	48 81 ec 78 22 00 00 	sub    $0x2278,%rsp
   99611:	48 89 94 24 08 01 00 	mov    %rdx,0x108(%rsp)
   99618:	00 
   99619:	0f b6 87 ae 04 00 00 	movzbl 0x4ae(%rdi),%eax
   99620:	88 44 24 37          	mov    %al,0x37(%rsp)
   99624:	66 0f ef c0          	pxor   %xmm0,%xmm0
   99628:	66 0f 7f 44 24 50    	movdqa %xmm0,0x50(%rsp)
   9962e:	48 c7 44 24 60 00 00 	movq   $0x0,0x60(%rsp)
   99635:	00 00 
   99637:	48 c7 84 24 b8 02 00 	movq   $0x1,0x2b8(%rsp)
   9963e:	00 01 00 00 00 
   99643:	48 c7 84 24 c0 02 00 	movq   $0x1,0x2c0(%rsp)
   9964a:	00 01 00 00 00 
   9964f:	48 c7 84 24 c8 02 00 	movq   $0x0,0x2c8(%rsp)
   99656:	00 00 00 00 00 
   9965b:	48 c7 84 24 20 05 00 	movq   $0x1,0x520(%rsp)
   99662:	00 01 00 00 00 
   99667:	f3 0f 7f 84 24 28 05 	movdqu %xmm0,0x528(%rsp)
   9966e:	00 00 
   99670:	48 c7 84 24 08 05 00 	movq   $0x2,0x508(%rsp)
   99677:	00 02 00 00 00 
   9967c:	f3 0f 7f 84 24 10 05 	movdqu %xmm0,0x510(%rsp)
   99683:	00 00 
   99685:	48 8b 87 b4 00 00 00 	mov    0xb4(%rdi),%rax
   9968c:	48 39 70 08          	cmp    %rsi,0x8(%rax)
   99690:	0f 8e bd 4e 00 00    	jle    9e553 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4f53>
   99696:	48 89 f5             	mov    %rsi,%rbp
   99699:	49 89 fe             	mov    %rdi,%r14
   9969c:	48 8d 87 5d 01 00 00 	lea    0x15d(%rdi),%rax
   996a3:	48 89 84 24 a0 00 00 	mov    %rax,0xa0(%rsp)
   996aa:	00 
   996ab:	48 89 f0             	mov    %rsi,%rax
   996ae:	48 c1 e0 06          	shl    $0x6,%rax
   996b2:	48 89 84 24 10 01 00 	mov    %rax,0x110(%rsp)
   996b9:	00 
   996ba:	48 c7 84 24 a8 00 00 	movq   $0xffffffffffffffff,0xa8(%rsp)
   996c1:	00 ff ff ff ff 
   996c6:	48 c7 44 24 40 00 00 	movq   $0x0,0x40(%rsp)
   996cd:	00 00 
   996cf:	48 c7 44 24 48 00 00 	movq   $0x0,0x48(%rsp)
   996d6:	00 00 
   996d8:	31 db                	xor    %ebx,%ebx
   996da:	48 c7 84 24 b0 00 00 	movq   $0xffffffffffffffff,0xb0(%rsp)
   996e1:	00 ff ff ff ff 
   996e6:	48 89 74 24 18       	mov    %rsi,0x18(%rsp)
   996eb:	48 89 7c 24 10       	mov    %rdi,0x10(%rsp)
   996f0:	eb 3f                	jmp    99731 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x131>
   996f2:	4c 8b 74 24 10       	mov    0x10(%rsp),%r14
   996f7:	4c 89 f7             	mov    %r14,%rdi
   996fa:	48 8d b4 24 60 04 00 	lea    0x460(%rsp),%rsi
   99701:	00 
   99702:	e8 d9 5d 03 00       	call   cf4e0 <_Wfl.fl_push$396d177d8e34f937>
   99707:	48 8b 6c 24 18       	mov    0x18(%rsp),%rbp
   9970c:	48 8b 5c 24 08       	mov    0x8(%rsp),%rbx
   99711:	66 66 66 66 66 66 2e 	data16 data16 data16 data16 data16 cs nopw 0x0(%rax,%rax,1)
   99718:	0f 1f 84 00 00 00 00 
   9971f:	00 
   99720:	49 8b 86 b4 00 00 00 	mov    0xb4(%r14),%rax
   99727:	48 39 68 08          	cmp    %rbp,0x8(%rax)
   9972b:	0f 8e 22 4e 00 00    	jle    9e553 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4f53>
   99731:	41 80 7e 50 00       	cmpb   $0x0,0x50(%r14)
   99736:	74 08                	je     99740 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x140>
   99738:	4c 89 f7             	mov    %r14,%rdi
   9973b:	e8 b0 e5 03 00       	call   d7cf0 <_Wfl.fl_gc_collect$edc7491238b27429>
   99740:	49 8b 86 c5 01 00 00 	mov    0x1c5(%r14),%rax
   99747:	48 85 c0             	test   %rax,%rax
   9974a:	74 45                	je     99791 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x191>
   9974c:	49 8b 8e bd 01 00 00 	mov    0x1bd(%r14),%rcx
   99753:	48 ff c1             	inc    %rcx
   99756:	0f 80 72 4e 00 00    	jo     9e5ce <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4fce>
   9975c:	49 89 8e bd 01 00 00 	mov    %rcx,0x1bd(%r14)
   99763:	48 39 c1             	cmp    %rax,%rcx
   99766:	7e 29                	jle    99791 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x191>
   99768:	ba 05 00 00 00       	mov    $0x5,%edx
   9976d:	41 b8 13 00 00 00    	mov    $0x13,%r8d
   99773:	4c 89 f7             	mov    %r14,%rdi
   99776:	48 8d 35 6a ae f7 ff 	lea    -0x85196(%rip),%rsi        # 145e7 <__abi_tag+0x142eb>
   9977d:	48 8d 0d 9c b0 f7 ff 	lea    -0x84f64(%rip),%rcx        # 14820 <__abi_tag+0x14524>
   99784:	4c 8d 8c 24 20 22 00 	lea    0x2220(%rsp),%r9
   9978b:	00 
   9978c:	e8 6f 50 07 00       	call   10e800 <_Wfl.fl_raise_bytes.bytesview.4$e8b5fa06fc8adc2d>
   99791:	41 80 be 5c 01 00 00 	cmpb   $0x0,0x15c(%r14)
   99798:	00 
   99799:	0f 85 86 18 00 00    	jne    9b025 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1a25>
   9979f:	49 8b 8e b4 00 00 00 	mov    0xb4(%r14),%rcx
   997a6:	48 8b 51 08          	mov    0x8(%rcx),%rdx
   997aa:	49 89 d5             	mov    %rdx,%r13
   997ad:	49 ff cd             	dec    %r13
   997b0:	0f 80 67 4e 00 00    	jo     9e61d <_Wfl.fl_vm_exec$a5081fe92876eb37+0x501d>
   997b6:	49 8b 06             	mov    (%r14),%rax
   997b9:	49 8b 76 52          	mov    0x52(%r14),%rsi
   997bd:	4c 3b ac 24 b0 00 00 	cmp    0xb0(%rsp),%r13
   997c4:	00 
   997c5:	4c 8b 7c 24 40       	mov    0x40(%rsp),%r15
   997ca:	48 8b 6c 24 48       	mov    0x48(%rsp),%rbp
   997cf:	75 0e                	jne    997df <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1df>
   997d1:	48 3b b4 24 a8 00 00 	cmp    0xa8(%rsp),%rsi
   997d8:	00 
   997d9:	0f 84 d3 00 00 00    	je     998b2 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x2b2>
   997df:	49 39 d5             	cmp    %rdx,%r13
   997e2:	0f 83 43 50 00 00    	jae    9e82b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x522b>
   997e8:	48 8b 39             	mov    (%rcx),%rdi
   997eb:	4d 89 e8             	mov    %r13,%r8
   997ee:	49 c1 e0 06          	shl    $0x6,%r8
   997f2:	4e 8b 1c 07          	mov    (%rdi,%r8,1),%r11
   997f6:	4d 8d 4b f4          	lea    -0xc(%r11),%r9
   997fa:	49 83 f9 f8          	cmp    $0xfffffffffffffff8,%r9
   997fe:	0f 82 22 4f 00 00    	jb     9e726 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5126>
   99804:	4a 8b 5c 07 08       	mov    0x8(%rdi,%r8,1),%rbx
   99809:	48 85 db             	test   %rbx,%rbx
   9980c:	0f 88 14 4f 00 00    	js     9e726 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5126>
   99812:	89 dd                	mov    %ebx,%ebp
   99814:	4c 8b 50 08          	mov    0x8(%rax),%r10
   99818:	4c 39 d5             	cmp    %r10,%rbp
   9981b:	0f 8d 05 4f 00 00    	jge    9e726 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5126>
   99821:	4c 8b 08             	mov    (%rax),%r9
   99824:	4c 69 fd 60 01 00 00 	imul   $0x160,%rbp,%r15
   9982b:	4f 39 1c 39          	cmp    %r11,(%r9,%r15,1)
   9982f:	0f 85 f1 4e 00 00    	jne    9e726 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5126>
   99835:	48 c1 eb 20          	shr    $0x20,%rbx
   99839:	4b 39 5c 39 19       	cmp    %rbx,0x19(%r9,%r15,1)
   9983e:	0f 85 e2 4e 00 00    	jne    9e726 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5126>
   99844:	4f 8b 9c 39 c9 00 00 	mov    0xc9(%r9,%r15,1),%r11
   9984b:	00 
   9984c:	49 8d 5b f4          	lea    -0xc(%r11),%rbx
   99850:	48 83 fb f8          	cmp    $0xfffffffffffffff8,%rbx
   99854:	0f 82 df 4e 00 00    	jb     9e739 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5139>
   9985a:	4b 8b 9c 39 d1 00 00 	mov    0xd1(%r9,%r15,1),%rbx
   99861:	00 
   99862:	48 85 db             	test   %rbx,%rbx
   99865:	0f 88 ce 4e 00 00    	js     9e739 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5139>
   9986b:	41 89 df             	mov    %ebx,%r15d
   9986e:	4d 39 d7             	cmp    %r10,%r15
   99871:	0f 83 c2 4e 00 00    	jae    9e739 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5139>
   99877:	4d 89 fc             	mov    %r15,%r12
   9987a:	4d 69 d7 60 01 00 00 	imul   $0x160,%r15,%r10
   99881:	4f 39 1c 11          	cmp    %r11,(%r9,%r10,1)
   99885:	0f 85 ae 4e 00 00    	jne    9e739 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5139>
   9988b:	48 c1 eb 20          	shr    $0x20,%rbx
   9988f:	4b 39 5c 11 19       	cmp    %rbx,0x19(%r9,%r10,1)
   99894:	0f 85 9f 4e 00 00    	jne    9e739 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5139>
   9989a:	4e 8b 7c 07 20       	mov    0x20(%rdi,%r8,1),%r15
   9989f:	4c 89 ac 24 b0 00 00 	mov    %r13,0xb0(%rsp)
   998a6:	00 
   998a7:	48 89 b4 24 a8 00 00 	mov    %rsi,0xa8(%rsp)
   998ae:	00 
   998af:	4c 89 e3             	mov    %r12,%rbx
   998b2:	49 39 d5             	cmp    %rdx,%r13
   998b5:	0f 83 e1 4e 00 00    	jae    9e79c <_Wfl.fl_vm_exec$a5081fe92876eb37+0x519c>
   998bb:	4c 89 ea             	mov    %r13,%rdx
   998be:	48 c1 e2 06          	shl    $0x6,%rdx
   998c2:	48 8b 09             	mov    (%rcx),%rcx
   998c5:	48 89 54 24 38       	mov    %rdx,0x38(%rsp)
   998ca:	4c 8b 44 11 18       	mov    0x18(%rcx,%rdx,1),%r8
   998cf:	80 7c 24 37 00       	cmpb   $0x0,0x37(%rsp)
   998d4:	48 89 5c 24 08       	mov    %rbx,0x8(%rsp)
   998d9:	4c 89 44 24 28       	mov    %r8,0x28(%rsp)
   998de:	4c 89 7c 24 40       	mov    %r15,0x40(%rsp)
   998e3:	48 89 6c 24 48       	mov    %rbp,0x48(%rsp)
   998e8:	0f 84 36 08 00 00    	je     9a124 <_Wfl.fl_vm_exec$a5081fe92876eb37+0xb24>
   998ee:	4d 85 c0             	test   %r8,%r8
   998f1:	78 1f                	js     99912 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x312>
   998f3:	48 3b 58 08          	cmp    0x8(%rax),%rbx
   998f7:	0f 83 6d 4f 00 00    	jae    9e86a <_Wfl.fl_vm_exec$a5081fe92876eb37+0x526a>
   998fd:	48 8b 00             	mov    (%rax),%rax
   99900:	48 69 cb 60 01 00 00 	imul   $0x160,%rbx,%rcx
   99907:	48 8b 44 08 61       	mov    0x61(%rax,%rcx,1),%rax
   9990c:	4c 3b 40 08          	cmp    0x8(%rax),%r8
   99910:	7c 28                	jl     9993a <_Wfl.fl_vm_exec$a5081fe92876eb37+0x33a>
   99912:	ba 0c 00 00 00       	mov    $0xc,%edx
   99917:	4c 89 f7             	mov    %r14,%rdi
   9991a:	48 8d 35 3e af f7 ff 	lea    -0x850c2(%rip),%rsi        # 1485f <__abi_tag+0x14563>
   99921:	48 89 d9             	mov    %rbx,%rcx
   99924:	45 31 c9             	xor    %r9d,%r9d
   99927:	6a 2c                	push   $0x2c
   99929:	48 8d 05 03 af f7 ff 	lea    -0x850fd(%rip),%rax        # 14833 <__abi_tag+0x14537>
   99930:	50                   	push   %rax
   99931:	e8 ca c4 05 00       	call   f5e00 <_Wfl.fl_vm_bug_at$494622488d4f7894>
   99936:	48 83 c4 10          	add    $0x10,%rsp
   9993a:	4c 89 f7             	mov    %r14,%rdi
   9993d:	4c 89 ee             	mov    %r13,%rsi
   99940:	48 89 da             	mov    %rbx,%rdx
   99943:	e8 c8 79 02 00       	call   c1310 <_Wfl.fl_read8$a2ddcd4dfd790b8d>
   99948:	49 89 c4             	mov    %rax,%r12
   9994b:	48 83 f8 3c          	cmp    $0x3c,%rax
   9994f:	7c 2d                	jl     9997e <_Wfl.fl_vm_exec$a5081fe92876eb37+0x37e>
   99951:	ba 0c 00 00 00       	mov    $0xc,%edx
   99956:	4c 89 f7             	mov    %r14,%rdi
   99959:	48 8d 35 ff ae f7 ff 	lea    -0x85101(%rip),%rsi        # 1485f <__abi_tag+0x14563>
   99960:	48 89 d9             	mov    %rbx,%rcx
   99963:	4c 8b 44 24 28       	mov    0x28(%rsp),%r8
   99968:	4d 89 e1             	mov    %r12,%r9
   9996b:	6a 15                	push   $0x15
   9996d:	48 8d 05 f7 ae f7 ff 	lea    -0x85109(%rip),%rax        # 1486b <__abi_tag+0x1456f>
   99974:	50                   	push   %rax
   99975:	e8 86 c4 05 00       	call   f5e00 <_Wfl.fl_vm_bug_at$494622488d4f7894>
   9997a:	48 83 c4 10          	add    $0x10,%rsp
   9997e:	49 8b 4e 20          	mov    0x20(%r14),%rcx
   99982:	49 8b 46 48          	mov    0x48(%r14),%rax
   99986:	41 0f 10 06          	movups (%r14),%xmm0
   9998a:	0f 29 84 24 d0 1a 00 	movaps %xmm0,0x1ad0(%rsp)
   99991:	00 
   99992:	41 0f 10 46 10       	movups 0x10(%r14),%xmm0
   99997:	0f 29 84 24 e0 1a 00 	movaps %xmm0,0x1ae0(%rsp)
   9999e:	00 
   9999f:	48 89 8c 24 f0 1a 00 	mov    %rcx,0x1af0(%rsp)
   999a6:	00 
   999a7:	41 0f b7 4e 50       	movzwl 0x50(%r14),%ecx
   999ac:	41 0f 10 46 28       	movups 0x28(%r14),%xmm0
   999b1:	0f 11 84 24 f8 1a 00 	movups %xmm0,0x1af8(%rsp)
   999b8:	00 
   999b9:	41 0f 10 46 38       	movups 0x38(%r14),%xmm0
   999be:	0f 11 84 24 08 1b 00 	movups %xmm0,0x1b08(%rsp)
   999c5:	00 
   999c6:	48 89 84 24 18 1b 00 	mov    %rax,0x1b18(%rsp)
   999cd:	00 
   999ce:	49 8b 46 62          	mov    0x62(%r14),%rax
   999d2:	48 89 8c 24 20 1b 00 	mov    %rcx,0x1b20(%rsp)
   999d9:	00 
   999da:	49 8b 8e 8a 00 00 00 	mov    0x8a(%r14),%rcx
   999e1:	41 0f 10 46 52       	movups 0x52(%r14),%xmm0
   999e6:	0f 11 84 24 28 1b 00 	movups %xmm0,0x1b28(%rsp)
   999ed:	00 
   999ee:	48 89 84 24 38 1b 00 	mov    %rax,0x1b38(%rsp)
   999f5:	00 
   999f6:	41 0f b7 86 a2 00 00 	movzwl 0xa2(%r14),%eax
   999fd:	00 
   999fe:	41 0f 10 46 6a       	movups 0x6a(%r14),%xmm0
   99a03:	0f 29 84 24 40 1b 00 	movaps %xmm0,0x1b40(%rsp)
   99a0a:	00 
   99a0b:	41 0f 10 46 7a       	movups 0x7a(%r14),%xmm0
   99a10:	0f 29 84 24 50 1b 00 	movaps %xmm0,0x1b50(%rsp)
   99a17:	00 
   99a18:	41 0f 10 86 92 00 00 	movups 0x92(%r14),%xmm0
   99a1f:	00 
   99a20:	48 89 8c 24 60 1b 00 	mov    %rcx,0x1b60(%rsp)
   99a27:	00 
   99a28:	0f 11 84 24 68 1b 00 	movups %xmm0,0x1b68(%rsp)
   99a2f:	00 
   99a30:	41 0f 10 86 a4 00 00 	movups 0xa4(%r14),%xmm0
   99a37:	00 
   99a38:	49 8b 8e c4 00 00 00 	mov    0xc4(%r14),%rcx
   99a3f:	48 89 84 24 78 1b 00 	mov    %rax,0x1b78(%rsp)
   99a46:	00 
   99a47:	41 0f b6 86 5c 01 00 	movzbl 0x15c(%r14),%eax
   99a4e:	00 
   99a4f:	0f 29 84 24 80 1b 00 	movaps %xmm0,0x1b80(%rsp)
   99a56:	00 
   99a57:	41 0f 10 86 b4 00 00 	movups 0xb4(%r14),%xmm0
   99a5e:	00 
   99a5f:	0f 29 84 24 90 1b 00 	movaps %xmm0,0x1b90(%rsp)
   99a66:	00 
   99a67:	41 0f 10 86 cc 00 00 	movups 0xcc(%r14),%xmm0
   99a6e:	00 
   99a6f:	48 89 8c 24 a0 1b 00 	mov    %rcx,0x1ba0(%rsp)
   99a76:	00 
   99a77:	0f 11 84 24 a8 1b 00 	movups %xmm0,0x1ba8(%rsp)
   99a7e:	00 
   99a7f:	41 0f 10 86 dc 00 00 	movups 0xdc(%r14),%xmm0
   99a86:	00 
   99a87:	0f 11 84 24 b8 1b 00 	movups %xmm0,0x1bb8(%rsp)
   99a8e:	00 
   99a8f:	41 0f 10 86 ec 00 00 	movups 0xec(%r14),%xmm0
   99a96:	00 
   99a97:	0f 11 84 24 c8 1b 00 	movups %xmm0,0x1bc8(%rsp)
   99a9e:	00 
   99a9f:	41 0f 10 86 fc 00 00 	movups 0xfc(%r14),%xmm0
   99aa6:	00 
   99aa7:	0f 11 84 24 d8 1b 00 	movups %xmm0,0x1bd8(%rsp)
   99aae:	00 
   99aaf:	41 0f 10 86 0c 01 00 	movups 0x10c(%r14),%xmm0
   99ab6:	00 
   99ab7:	0f 11 84 24 e8 1b 00 	movups %xmm0,0x1be8(%rsp)
   99abe:	00 
   99abf:	41 0f 10 86 1c 01 00 	movups 0x11c(%r14),%xmm0
   99ac6:	00 
   99ac7:	0f 11 84 24 f8 1b 00 	movups %xmm0,0x1bf8(%rsp)
   99ace:	00 
   99acf:	41 0f 10 86 2c 01 00 	movups 0x12c(%r14),%xmm0
   99ad6:	00 
   99ad7:	0f 11 84 24 08 1c 00 	movups %xmm0,0x1c08(%rsp)
   99ade:	00 
   99adf:	41 0f 10 86 3c 01 00 	movups 0x13c(%r14),%xmm0
   99ae6:	00 
   99ae7:	0f 11 84 24 18 1c 00 	movups %xmm0,0x1c18(%rsp)
   99aee:	00 
   99aef:	41 0f 10 86 4c 01 00 	movups 0x14c(%r14),%xmm0
   99af6:	00 
   99af7:	0f 11 84 24 28 1c 00 	movups %xmm0,0x1c28(%rsp)
   99afe:	00 
   99aff:	41 0f 10 86 5d 01 00 	movups 0x15d(%r14),%xmm0
   99b06:	00 
   99b07:	88 84 24 38 1c 00 00 	mov    %al,0x1c38(%rsp)
   99b0e:	0f 29 84 24 40 1c 00 	movaps %xmm0,0x1c40(%rsp)
   99b15:	00 
   99b16:	41 0f 10 86 6d 01 00 	movups 0x16d(%r14),%xmm0
   99b1d:	00 
   99b1e:	41 0f b6 86 dd 01 00 	movzbl 0x1dd(%r14),%eax
   99b25:	00 
   99b26:	49 8b 8e ee 01 00 00 	mov    0x1ee(%r14),%rcx
   99b2d:	0f 29 84 24 50 1c 00 	movaps %xmm0,0x1c50(%rsp)
   99b34:	00 
   99b35:	41 0f 10 86 7d 01 00 	movups 0x17d(%r14),%xmm0
   99b3c:	00 
   99b3d:	0f 29 84 24 60 1c 00 	movaps %xmm0,0x1c60(%rsp)
   99b44:	00 
   99b45:	41 0f 10 86 8d 01 00 	movups 0x18d(%r14),%xmm0
   99b4c:	00 
   99b4d:	0f 29 84 24 70 1c 00 	movaps %xmm0,0x1c70(%rsp)
   99b54:	00 
   99b55:	41 0f 10 86 9d 01 00 	movups 0x19d(%r14),%xmm0
   99b5c:	00 
   99b5d:	0f 29 84 24 80 1c 00 	movaps %xmm0,0x1c80(%rsp)
   99b64:	00 
   99b65:	41 0f 10 86 ad 01 00 	movups 0x1ad(%r14),%xmm0
   99b6c:	00 
   99b6d:	0f 29 84 24 90 1c 00 	movaps %xmm0,0x1c90(%rsp)
   99b74:	00 
   99b75:	41 0f 10 86 bd 01 00 	movups 0x1bd(%r14),%xmm0
   99b7c:	00 
   99b7d:	0f 29 84 24 a0 1c 00 	movaps %xmm0,0x1ca0(%rsp)
   99b84:	00 
   99b85:	41 0f 10 86 cd 01 00 	movups 0x1cd(%r14),%xmm0
   99b8c:	00 
   99b8d:	0f 29 84 24 b0 1c 00 	movaps %xmm0,0x1cb0(%rsp)
   99b94:	00 
   99b95:	88 84 24 c0 1c 00 00 	mov    %al,0x1cc0(%rsp)
   99b9c:	49 8b 86 16 02 00 00 	mov    0x216(%r14),%rax
   99ba3:	41 0f 10 86 de 01 00 	movups 0x1de(%r14),%xmm0
   99baa:	00 
   99bab:	0f 11 84 24 c8 1c 00 	movups %xmm0,0x1cc8(%rsp)
   99bb2:	00 
   99bb3:	48 89 8c 24 d8 1c 00 	mov    %rcx,0x1cd8(%rsp)
   99bba:	00 
   99bbb:	41 0f b6 8e 1e 02 00 	movzbl 0x21e(%r14),%ecx
   99bc2:	00 
   99bc3:	41 0f 10 86 f6 01 00 	movups 0x1f6(%r14),%xmm0
   99bca:	00 
   99bcb:	0f 29 84 24 e0 1c 00 	movaps %xmm0,0x1ce0(%rsp)
   99bd2:	00 
   99bd3:	41 0f 10 86 06 02 00 	movups 0x206(%r14),%xmm0
   99bda:	00 
   99bdb:	0f 29 84 24 f0 1c 00 	movaps %xmm0,0x1cf0(%rsp)
   99be2:	00 
   99be3:	48 89 84 24 00 1d 00 	mov    %rax,0x1d00(%rsp)
   99bea:	00 
   99beb:	49 8b 86 2f 02 00 00 	mov    0x22f(%r14),%rax
   99bf2:	48 89 8c 24 08 1d 00 	mov    %rcx,0x1d08(%rsp)
   99bf9:	00 
   99bfa:	49 8b 8e 47 02 00 00 	mov    0x247(%r14),%rcx
   99c01:	41 0f 10 86 1f 02 00 	movups 0x21f(%r14),%xmm0
   99c08:	00 
   99c09:	0f 29 84 24 10 1d 00 	movaps %xmm0,0x1d10(%rsp)
   99c10:	00 
   99c11:	48 89 84 24 20 1d 00 	mov    %rax,0x1d20(%rsp)
   99c18:	00 
   99c19:	49 8b 86 7f 02 00 00 	mov    0x27f(%r14),%rax
   99c20:	41 0f 10 86 37 02 00 	movups 0x237(%r14),%xmm0
   99c27:	00 
   99c28:	0f 11 84 24 28 1d 00 	movups %xmm0,0x1d28(%rsp)
   99c2f:	00 
   99c30:	41 0f 10 86 4f 02 00 	movups 0x24f(%r14),%xmm0
   99c37:	00 
   99c38:	48 89 8c 24 38 1d 00 	mov    %rcx,0x1d38(%rsp)
   99c3f:	00 
   99c40:	0f 29 84 24 40 1d 00 	movaps %xmm0,0x1d40(%rsp)
   99c47:	00 
   99c48:	41 0f 10 86 5f 02 00 	movups 0x25f(%r14),%xmm0
   99c4f:	00 
   99c50:	0f 29 84 24 50 1d 00 	movaps %xmm0,0x1d50(%rsp)
   99c57:	00 
   99c58:	41 0f 10 86 6f 02 00 	movups 0x26f(%r14),%xmm0
   99c5f:	00 
   99c60:	0f 29 84 24 60 1d 00 	movaps %xmm0,0x1d60(%rsp)
   99c67:	00 
   99c68:	41 0f 10 86 87 02 00 	movups 0x287(%r14),%xmm0
   99c6f:	00 
   99c70:	48 89 84 24 70 1d 00 	mov    %rax,0x1d70(%rsp)
   99c77:	00 
   99c78:	0f 11 84 24 78 1d 00 	movups %xmm0,0x1d78(%rsp)
   99c7f:	00 
   99c80:	41 0f 10 86 97 02 00 	movups 0x297(%r14),%xmm0
   99c87:	00 
   99c88:	0f 11 84 24 88 1d 00 	movups %xmm0,0x1d88(%rsp)
   99c8f:	00 
   99c90:	41 0f 10 86 a7 02 00 	movups 0x2a7(%r14),%xmm0
   99c97:	00 
   99c98:	0f 11 84 24 98 1d 00 	movups %xmm0,0x1d98(%rsp)
   99c9f:	00 
   99ca0:	41 0f 10 86 b7 02 00 	movups 0x2b7(%r14),%xmm0
   99ca7:	00 
   99ca8:	0f 11 84 24 a8 1d 00 	movups %xmm0,0x1da8(%rsp)
   99caf:	00 
   99cb0:	41 0f 10 86 c7 02 00 	movups 0x2c7(%r14),%xmm0
   99cb7:	00 
   99cb8:	0f 11 84 24 b8 1d 00 	movups %xmm0,0x1db8(%rsp)
   99cbf:	00 
   99cc0:	41 0f 10 86 d7 02 00 	movups 0x2d7(%r14),%xmm0
   99cc7:	00 
   99cc8:	0f 11 84 24 c8 1d 00 	movups %xmm0,0x1dc8(%rsp)
   99ccf:	00 
   99cd0:	41 0f 10 86 e7 02 00 	movups 0x2e7(%r14),%xmm0
   99cd7:	00 
   99cd8:	0f 11 84 24 d8 1d 00 	movups %xmm0,0x1dd8(%rsp)
   99cdf:	00 
   99ce0:	41 0f 10 86 f7 02 00 	movups 0x2f7(%r14),%xmm0
   99ce7:	00 
   99ce8:	0f 11 84 24 e8 1d 00 	movups %xmm0,0x1de8(%rsp)
   99cef:	00 
   99cf0:	41 0f 10 86 07 03 00 	movups 0x307(%r14),%xmm0
   99cf7:	00 
   99cf8:	49 8b 86 17 03 00 00 	mov    0x317(%r14),%rax
   99cff:	49 8b 8e 3f 03 00 00 	mov    0x33f(%r14),%rcx
   99d06:	0f 11 84 24 f8 1d 00 	movups %xmm0,0x1df8(%rsp)
   99d0d:	00 
   99d0e:	48 89 84 24 08 1e 00 	mov    %rax,0x1e08(%rsp)
   99d15:	00 
   99d16:	49 8b 86 57 03 00 00 	mov    0x357(%r14),%rax
   99d1d:	41 0f 10 86 1f 03 00 	movups 0x31f(%r14),%xmm0
   99d24:	00 
   99d25:	0f 29 84 24 10 1e 00 	movaps %xmm0,0x1e10(%rsp)
   99d2c:	00 
   99d2d:	41 0f 10 86 2f 03 00 	movups 0x32f(%r14),%xmm0
   99d34:	00 
   99d35:	0f 29 84 24 20 1e 00 	movaps %xmm0,0x1e20(%rsp)
   99d3c:	00 
   99d3d:	48 89 8c 24 30 1e 00 	mov    %rcx,0x1e30(%rsp)
   99d44:	00 
   99d45:	41 0f b6 8e 5f 03 00 	movzbl 0x35f(%r14),%ecx
   99d4c:	00 
   99d4d:	41 0f 10 86 47 03 00 	movups 0x347(%r14),%xmm0
   99d54:	00 
   99d55:	0f 11 84 24 38 1e 00 	movups %xmm0,0x1e38(%rsp)
   99d5c:	00 
   99d5d:	48 89 84 24 48 1e 00 	mov    %rax,0x1e48(%rsp)
   99d64:	00 
   99d65:	49 8b 86 90 03 00 00 	mov    0x390(%r14),%rax
   99d6c:	48 89 8c 24 50 1e 00 	mov    %rcx,0x1e50(%rsp)
   99d73:	00 
   99d74:	41 0f b6 8e a8 03 00 	movzbl 0x3a8(%r14),%ecx
   99d7b:	00 
   99d7c:	41 0f 10 86 60 03 00 	movups 0x360(%r14),%xmm0
   99d83:	00 
   99d84:	0f 11 84 24 58 1e 00 	movups %xmm0,0x1e58(%rsp)
   99d8b:	00 
   99d8c:	41 0f 10 86 70 03 00 	movups 0x370(%r14),%xmm0
   99d93:	00 
   99d94:	0f 11 84 24 68 1e 00 	movups %xmm0,0x1e68(%rsp)
   99d9b:	00 
   99d9c:	41 0f 10 86 80 03 00 	movups 0x380(%r14),%xmm0
   99da3:	00 
   99da4:	0f 11 84 24 78 1e 00 	movups %xmm0,0x1e78(%rsp)
   99dab:	00 
   99dac:	48 89 84 24 88 1e 00 	mov    %rax,0x1e88(%rsp)
   99db3:	00 
   99db4:	49 8b 86 b9 03 00 00 	mov    0x3b9(%r14),%rax
   99dbb:	41 0f 10 86 98 03 00 	movups 0x398(%r14),%xmm0
   99dc2:	00 
   99dc3:	0f 29 84 24 90 1e 00 	movaps %xmm0,0x1e90(%rsp)
   99dca:	00 
   99dcb:	48 89 8c 24 a0 1e 00 	mov    %rcx,0x1ea0(%rsp)
   99dd2:	00 
   99dd3:	49 8b 8e 01 04 00 00 	mov    0x401(%r14),%rcx
   99dda:	41 0f 10 86 a9 03 00 	movups 0x3a9(%r14),%xmm0
   99de1:	00 
   99de2:	0f 11 84 24 a8 1e 00 	movups %xmm0,0x1ea8(%rsp)
   99de9:	00 
   99dea:	41 0f 10 86 c1 03 00 	movups 0x3c1(%r14),%xmm0
   99df1:	00 
   99df2:	48 89 84 24 b8 1e 00 	mov    %rax,0x1eb8(%rsp)
   99df9:	00 
   99dfa:	0f 29 84 24 c0 1e 00 	movaps %xmm0,0x1ec0(%rsp)
   99e01:	00 
   99e02:	41 0f 10 86 d1 03 00 	movups 0x3d1(%r14),%xmm0
   99e09:	00 
   99e0a:	0f 29 84 24 d0 1e 00 	movaps %xmm0,0x1ed0(%rsp)
   99e11:	00 
   99e12:	41 0f 10 86 e1 03 00 	movups 0x3e1(%r14),%xmm0
   99e19:	00 
   99e1a:	0f 29 84 24 e0 1e 00 	movaps %xmm0,0x1ee0(%rsp)
   99e21:	00 
   99e22:	41 0f 10 86 f1 03 00 	movups 0x3f1(%r14),%xmm0
   99e29:	00 
   99e2a:	0f 29 84 24 f0 1e 00 	movaps %xmm0,0x1ef0(%rsp)
   99e31:	00 
   99e32:	41 0f 10 86 09 04 00 	movups 0x409(%r14),%xmm0
   99e39:	00 
   99e3a:	48 89 8c 24 00 1f 00 	mov    %rcx,0x1f00(%rsp)
   99e41:	00 
   99e42:	0f 11 84 24 08 1f 00 	movups %xmm0,0x1f08(%rsp)
   99e49:	00 
   99e4a:	41 0f 10 86 19 04 00 	movups 0x419(%r14),%xmm0
   99e51:	00 
   99e52:	41 0f b6 86 69 04 00 	movzbl 0x469(%r14),%eax
   99e59:	00 
   99e5a:	41 0f b7 8e 6a 04 00 	movzwl 0x46a(%r14),%ecx
   99e61:	00 
   99e62:	c1 e1 08             	shl    $0x8,%ecx
   99e65:	48 09 c1             	or     %rax,%rcx
   99e68:	49 8b 86 49 04 00 00 	mov    0x449(%r14),%rax
   99e6f:	0f 11 84 24 18 1f 00 	movups %xmm0,0x1f18(%rsp)
   99e76:	00 
   99e77:	41 0f 10 86 29 04 00 	movups 0x429(%r14),%xmm0
   99e7e:	00 
   99e7f:	0f 11 84 24 28 1f 00 	movups %xmm0,0x1f28(%rsp)
   99e86:	00 
   99e87:	41 0f 10 86 39 04 00 	movups 0x439(%r14),%xmm0
   99e8e:	00 
   99e8f:	0f 11 84 24 38 1f 00 	movups %xmm0,0x1f38(%rsp)
   99e96:	00 
   99e97:	48 89 84 24 48 1f 00 	mov    %rax,0x1f48(%rsp)
   99e9e:	00 
   99e9f:	49 8b 86 61 04 00 00 	mov    0x461(%r14),%rax
   99ea6:	41 0f 10 86 51 04 00 	movups 0x451(%r14),%xmm0
   99ead:	00 
   99eae:	0f 29 84 24 50 1f 00 	movaps %xmm0,0x1f50(%rsp)
   99eb5:	00 
   99eb6:	48 89 84 24 60 1f 00 	mov    %rax,0x1f60(%rsp)
   99ebd:	00 
   99ebe:	41 0f 10 86 6c 04 00 	movups 0x46c(%r14),%xmm0
   99ec5:	00 
   99ec6:	48 89 8c 24 68 1f 00 	mov    %rcx,0x1f68(%rsp)
   99ecd:	00 
   99ece:	0f 29 84 24 70 1f 00 	movaps %xmm0,0x1f70(%rsp)
   99ed5:	00 
   99ed6:	41 0f 10 86 7c 04 00 	movups 0x47c(%r14),%xmm0
   99edd:	00 
   99ede:	41 0f b7 86 8c 04 00 	movzwl 0x48c(%r14),%eax
   99ee5:	00 
   99ee6:	0f 29 84 24 80 1f 00 	movaps %xmm0,0x1f80(%rsp)
   99eed:	00 
   99eee:	41 0f 10 86 8e 04 00 	movups 0x48e(%r14),%xmm0
   99ef5:	00 
   99ef6:	48 89 84 24 90 1f 00 	mov    %rax,0x1f90(%rsp)
   99efd:	00 
   99efe:	0f 11 84 24 98 1f 00 	movups %xmm0,0x1f98(%rsp)
   99f05:	00 
   99f06:	41 0f 10 86 9e 04 00 	movups 0x49e(%r14),%xmm0
   99f0d:	00 
   99f0e:	41 0f b6 86 ae 04 00 	movzbl 0x4ae(%r14),%eax
   99f15:	00 
   99f16:	0f 11 84 24 a8 1f 00 	movups %xmm0,0x1fa8(%rsp)
   99f1d:	00 
   99f1e:	41 0f 10 86 af 04 00 	movups 0x4af(%r14),%xmm0
   99f25:	00 
   99f26:	88 84 24 b8 1f 00 00 	mov    %al,0x1fb8(%rsp)
   99f2d:	0f 29 84 24 c0 1f 00 	movaps %xmm0,0x1fc0(%rsp)
   99f34:	00 
   99f35:	41 0f 10 86 bf 04 00 	movups 0x4bf(%r14),%xmm0
   99f3c:	00 
   99f3d:	0f 29 84 24 d0 1f 00 	movaps %xmm0,0x1fd0(%rsp)
   99f44:	00 
   99f45:	f3 41 0f 6f 86 cf 04 	movdqu 0x4cf(%r14),%xmm0
   99f4c:	00 00 
   99f4e:	66 0f 7f 84 24 e0 1f 	movdqa %xmm0,0x1fe0(%rsp)
   99f55:	00 00 
   99f57:	49 8b 86 df 04 00 00 	mov    0x4df(%r14),%rax
   99f5e:	48 89 84 24 f0 1f 00 	mov    %rax,0x1ff0(%rsp)
   99f65:	00 
   99f66:	48 89 de             	mov    %rbx,%rsi
   99f69:	49 8b 1e             	mov    (%r14),%rbx
   99f6c:	48 8d bc 24 d0 1a 00 	lea    0x1ad0(%rsp),%rdi
   99f73:	00 
   99f74:	4c 8b 7c 24 28       	mov    0x28(%rsp),%r15
   99f79:	4c 89 fa             	mov    %r15,%rdx
   99f7c:	e8 ef 1a 07 00       	call   10ba70 <_Wfl.check_op_len$52db95ddc731f12e>
   99f81:	4d 89 f8             	mov    %r15,%r8
   99f84:	84 d2                	test   %dl,%dl
   99f86:	74 2d                	je     99fb5 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x9b5>
   99f88:	4c 01 c0             	add    %r8,%rax
   99f8b:	0f 80 c3 46 00 00    	jo     9e654 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5054>
   99f91:	48 8b 54 24 08       	mov    0x8(%rsp),%rdx
   99f96:	48 3b 53 08          	cmp    0x8(%rbx),%rdx
   99f9a:	0f 83 eb 48 00 00    	jae    9e88b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x528b>
   99fa0:	48 8b 0b             	mov    (%rbx),%rcx
   99fa3:	48 69 d2 60 01 00 00 	imul   $0x160,%rdx,%rdx
   99faa:	48 8b 4c 11 61       	mov    0x61(%rcx,%rdx,1),%rcx
   99faf:	48 3b 41 08          	cmp    0x8(%rcx),%rax
   99fb3:	7e 2f                	jle    99fe4 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x9e4>
   99fb5:	ba 0c 00 00 00       	mov    $0xc,%edx
   99fba:	4c 89 f7             	mov    %r14,%rdi
   99fbd:	48 8d 35 9b a8 f7 ff 	lea    -0x85765(%rip),%rsi        # 1485f <__abi_tag+0x14563>
   99fc4:	48 8b 4c 24 08       	mov    0x8(%rsp),%rcx
   99fc9:	4d 89 e1             	mov    %r12,%r9
   99fcc:	6a 1c                	push   $0x1c
   99fce:	48 8d 05 ab a8 f7 ff 	lea    -0x85755(%rip),%rax        # 14880 <__abi_tag+0x14584>
   99fd5:	50                   	push   %rax
   99fd6:	e8 25 be 05 00       	call   f5e00 <_Wfl.fl_vm_bug_at$494622488d4f7894>
   99fdb:	4c 8b 44 24 38       	mov    0x38(%rsp),%r8
   99fe0:	48 83 c4 10          	add    $0x10,%rsp
   99fe4:	49 83 fc 32          	cmp    $0x32,%r12
   99fe8:	48 8b 5c 24 08       	mov    0x8(%rsp),%rbx
   99fed:	4c 8b 7c 24 40       	mov    0x40(%rsp),%r15
   99ff2:	0f 87 a7 00 00 00    	ja     9a09f <_Wfl.fl_vm_exec$a5081fe92876eb37+0xa9f>
   99ff8:	48 b8 01 c0 01 00 80 	movabs $0x430800001c001,%rax
   99fff:	30 04 00 
   9a002:	4c 0f a3 e0          	bt     %r12,%rax
   9a006:	0f 83 93 00 00 00    	jae    9a09f <_Wfl.fl_vm_exec$a5081fe92876eb37+0xa9f>
   9a00c:	4c 89 c0             	mov    %r8,%rax
   9a00f:	48 ff c0             	inc    %rax
   9a012:	0f 80 ef 45 00 00    	jo     9e607 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5007>
   9a018:	49 8b 0e             	mov    (%r14),%rcx
   9a01b:	48 3b 59 08          	cmp    0x8(%rcx),%rbx
   9a01f:	0f 83 35 47 00 00    	jae    9e75a <_Wfl.fl_vm_exec$a5081fe92876eb37+0x515a>
   9a025:	48 8b 09             	mov    (%rcx),%rcx
   9a028:	48 69 d3 60 01 00 00 	imul   $0x160,%rbx,%rdx
   9a02f:	48 8b 74 11 61       	mov    0x61(%rcx,%rdx,1),%rsi
   9a034:	4c 8b 4e 08          	mov    0x8(%rsi),%r9
   9a038:	4c 39 c8             	cmp    %r9,%rax
   9a03b:	0f 83 19 47 00 00    	jae    9e75a <_Wfl.fl_vm_exec$a5081fe92876eb37+0x515a>
   9a041:	48 89 c7             	mov    %rax,%rdi
   9a044:	48 ff c7             	inc    %rdi
   9a047:	0f 80 ba 45 00 00    	jo     9e607 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5007>
   9a04d:	4c 39 cf             	cmp    %r9,%rdi
   9a050:	0f 83 04 47 00 00    	jae    9e75a <_Wfl.fl_vm_exec$a5081fe92876eb37+0x515a>
   9a056:	48 8b 36             	mov    (%rsi),%rsi
   9a059:	0f b6 04 06          	movzbl (%rsi,%rax,1),%eax
   9a05d:	0f b6 34 3e          	movzbl (%rsi,%rdi,1),%esi
   9a061:	c1 e6 08             	shl    $0x8,%esi
   9a064:	48 09 c6             	or     %rax,%rsi
   9a067:	48 8b 44 11 29       	mov    0x29(%rcx,%rdx,1),%rax
   9a06c:	48 3b 70 08          	cmp    0x8(%rax),%rsi
   9a070:	7c 2d                	jl     9a09f <_Wfl.fl_vm_exec$a5081fe92876eb37+0xa9f>
   9a072:	ba 0c 00 00 00       	mov    $0xc,%edx
   9a077:	4c 89 f7             	mov    %r14,%rdi
   9a07a:	48 8d 35 de a7 f7 ff 	lea    -0x85822(%rip),%rsi        # 1485f <__abi_tag+0x14563>
   9a081:	48 89 d9             	mov    %rbx,%rcx
   9a084:	4d 89 e1             	mov    %r12,%r9
   9a087:	6a 22                	push   $0x22
   9a089:	48 8d 05 0c a8 f7 ff 	lea    -0x857f4(%rip),%rax        # 1489c <__abi_tag+0x145a0>
   9a090:	50                   	push   %rax
   9a091:	e8 6a bd 05 00       	call   f5e00 <_Wfl.fl_vm_bug_at$494622488d4f7894>
   9a096:	4c 8b 44 24 38       	mov    0x38(%rsp),%r8
   9a09b:	48 83 c4 10          	add    $0x10,%rsp
   9a09f:	49 8d 44 24 f7       	lea    -0x9(%r12),%rax
   9a0a4:	48 83 f8 01          	cmp    $0x1,%rax
   9a0a8:	0f 87 8e 00 00 00    	ja     9a13c <_Wfl.fl_vm_exec$a5081fe92876eb37+0xb3c>
   9a0ae:	49 8b 0e             	mov    (%r14),%rcx
   9a0b1:	48 3b 59 08          	cmp    0x8(%rcx),%rbx
   9a0b5:	0f 83 5a 47 00 00    	jae    9e815 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5215>
   9a0bb:	4c 89 c0             	mov    %r8,%rax
   9a0be:	48 ff c0             	inc    %rax
   9a0c1:	0f 80 77 45 00 00    	jo     9e63e <_Wfl.fl_vm_exec$a5081fe92876eb37+0x503e>
   9a0c7:	48 8b 11             	mov    (%rcx),%rdx
   9a0ca:	48 69 f3 60 01 00 00 	imul   $0x160,%rbx,%rsi
   9a0d1:	48 8b 4c 32 61       	mov    0x61(%rdx,%rsi,1),%rcx
   9a0d6:	48 3b 41 08          	cmp    0x8(%rcx),%rax
   9a0da:	0f 83 35 47 00 00    	jae    9e815 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5215>
   9a0e0:	48 8b 94 32 99 00 00 	mov    0x99(%rdx,%rsi,1),%rdx
   9a0e7:	00 
   9a0e8:	48 ff c2             	inc    %rdx
   9a0eb:	0f 80 4d 45 00 00    	jo     9e63e <_Wfl.fl_vm_exec$a5081fe92876eb37+0x503e>
   9a0f1:	48 8b 09             	mov    (%rcx),%rcx
   9a0f4:	0f b6 04 01          	movzbl (%rcx,%rax,1),%eax
   9a0f8:	48 39 c2             	cmp    %rax,%rdx
   9a0fb:	0f 8f d0 00 00 00    	jg     9a1d1 <_Wfl.fl_vm_exec$a5081fe92876eb37+0xbd1>
   9a101:	ba 0c 00 00 00       	mov    $0xc,%edx
   9a106:	4c 89 f7             	mov    %r14,%rdi
   9a109:	48 8d 35 4f a7 f7 ff 	lea    -0x858b1(%rip),%rsi        # 1485f <__abi_tag+0x14563>
   9a110:	48 89 d9             	mov    %rbx,%rcx
   9a113:	4d 89 e1             	mov    %r12,%r9
   9a116:	6a 23                	push   $0x23
   9a118:	48 8d 05 9f a7 f7 ff 	lea    -0x85861(%rip),%rax        # 148be <__abi_tag+0x145c2>
   9a11f:	e9 9e 00 00 00       	jmp    9a1c2 <_Wfl.fl_vm_exec$a5081fe92876eb37+0xbc2>
   9a124:	4c 89 f7             	mov    %r14,%rdi
   9a127:	4c 89 ee             	mov    %r13,%rsi
   9a12a:	48 89 da             	mov    %rbx,%rdx
   9a12d:	e8 de 71 02 00       	call   c1310 <_Wfl.fl_read8$a2ddcd4dfd790b8d>
   9a132:	48 89 44 24 20       	mov    %rax,0x20(%rsp)
   9a137:	e9 4a 01 00 00       	jmp    9a286 <_Wfl.fl_vm_exec$a5081fe92876eb37+0xc86>
   9a13c:	4c 89 e0             	mov    %r12,%rax
   9a13f:	48 83 c0 f5          	add    $0xfffffffffffffff5,%rax
   9a143:	48 83 f8 01          	cmp    $0x1,%rax
   9a147:	0f 87 84 00 00 00    	ja     9a1d1 <_Wfl.fl_vm_exec$a5081fe92876eb37+0xbd1>
   9a14d:	49 8b 16             	mov    (%r14),%rdx
   9a150:	48 8b 42 08          	mov    0x8(%rdx),%rax
   9a154:	48 39 c3             	cmp    %rax,%rbx
   9a157:	0f 83 f7 46 00 00    	jae    9e854 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5254>
   9a15d:	4c 89 c1             	mov    %r8,%rcx
   9a160:	48 ff c1             	inc    %rcx
   9a163:	0f 80 0c 45 00 00    	jo     9e675 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5075>
   9a169:	48 8b 12             	mov    (%rdx),%rdx
   9a16c:	48 69 f3 60 01 00 00 	imul   $0x160,%rbx,%rsi
   9a173:	48 8b 74 32 61       	mov    0x61(%rdx,%rsi,1),%rsi
   9a178:	48 3b 4e 08          	cmp    0x8(%rsi),%rcx
   9a17c:	0f 83 d2 46 00 00    	jae    9e854 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5254>
   9a182:	48 39 c5             	cmp    %rax,%rbp
   9a185:	0f 83 c9 46 00 00    	jae    9e854 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5254>
   9a18b:	48 8b 06             	mov    (%rsi),%rax
   9a18e:	0f b6 04 08          	movzbl (%rax,%rcx,1),%eax
   9a192:	48 69 cd 60 01 00 00 	imul   $0x160,%rbp,%rcx
   9a199:	48 8b 4c 0a 29       	mov    0x29(%rdx,%rcx,1),%rcx
   9a19e:	48 39 41 08          	cmp    %rax,0x8(%rcx)
   9a1a2:	7f 2d                	jg     9a1d1 <_Wfl.fl_vm_exec$a5081fe92876eb37+0xbd1>
   9a1a4:	ba 0c 00 00 00       	mov    $0xc,%edx
   9a1a9:	4c 89 f7             	mov    %r14,%rdi
   9a1ac:	48 8d 35 ac a6 f7 ff 	lea    -0x85954(%rip),%rsi        # 1485f <__abi_tag+0x14563>
   9a1b3:	48 89 d9             	mov    %rbx,%rcx
   9a1b6:	4d 89 e1             	mov    %r12,%r9
   9a1b9:	6a 21                	push   $0x21
   9a1bb:	48 8d 05 1f a7 f7 ff 	lea    -0x858e1(%rip),%rax        # 148e1 <__abi_tag+0x145e5>
   9a1c2:	50                   	push   %rax
   9a1c3:	e8 38 bc 05 00       	call   f5e00 <_Wfl.fl_vm_bug_at$494622488d4f7894>
   9a1c8:	4c 8b 44 24 38       	mov    0x38(%rsp),%r8
   9a1cd:	48 83 c4 10          	add    $0x10,%rsp
   9a1d1:	49 8b 86 ac 00 00 00 	mov    0xac(%r14),%rax
   9a1d8:	4c 39 f8             	cmp    %r15,%rax
   9a1db:	7d 34                	jge    9a211 <_Wfl.fl_vm_exec$a5081fe92876eb37+0xc11>
   9a1dd:	ba 0c 00 00 00       	mov    $0xc,%edx
   9a1e2:	4c 89 f7             	mov    %r14,%rdi
   9a1e5:	48 8d 35 73 a6 f7 ff 	lea    -0x8598d(%rip),%rsi        # 1485f <__abi_tag+0x14563>
   9a1ec:	48 89 d9             	mov    %rbx,%rcx
   9a1ef:	4d 89 e1             	mov    %r12,%r9
   9a1f2:	6a 16                	push   $0x16
   9a1f4:	48 8d 05 07 a7 f7 ff 	lea    -0x858f9(%rip),%rax        # 14902 <__abi_tag+0x14606>
   9a1fb:	50                   	push   %rax
   9a1fc:	e8 ff bb 05 00       	call   f5e00 <_Wfl.fl_vm_bug_at$494622488d4f7894>
   9a201:	4c 8b 44 24 38       	mov    0x38(%rsp),%r8
   9a206:	48 83 c4 10          	add    $0x10,%rsp
   9a20a:	49 8b 86 ac 00 00 00 	mov    0xac(%r14),%rax
   9a211:	49 8b 8e a4 00 00 00 	mov    0xa4(%r14),%rcx
   9a218:	48 3b 41 08          	cmp    0x8(%rcx),%rax
   9a21c:	7e 2d                	jle    9a24b <_Wfl.fl_vm_exec$a5081fe92876eb37+0xc4b>
   9a21e:	ba 0c 00 00 00       	mov    $0xc,%edx
   9a223:	4c 89 f7             	mov    %r14,%rdi
   9a226:	48 8d 35 32 a6 f7 ff 	lea    -0x859ce(%rip),%rsi        # 1485f <__abi_tag+0x14563>
   9a22d:	48 89 d9             	mov    %rbx,%rcx
   9a230:	4d 89 e1             	mov    %r12,%r9
   9a233:	6a 21                	push   $0x21
   9a235:	48 8d 05 dc a6 f7 ff 	lea    -0x85924(%rip),%rax        # 14918 <__abi_tag+0x1461c>
   9a23c:	50                   	push   %rax
   9a23d:	e8 be bb 05 00       	call   f5e00 <_Wfl.fl_vm_bug_at$494622488d4f7894>
   9a242:	4c 8b 44 24 38       	mov    0x38(%rsp),%r8
   9a247:	48 83 c4 10          	add    $0x10,%rsp
   9a24b:	49 8b 86 bc 00 00 00 	mov    0xbc(%r14),%rax
   9a252:	48 83 78 08 41       	cmpq   $0x41,0x8(%rax)
   9a257:	4c 89 64 24 20       	mov    %r12,0x20(%rsp)
   9a25c:	7c 28                	jl     9a286 <_Wfl.fl_vm_exec$a5081fe92876eb37+0xc86>
   9a25e:	ba 0c 00 00 00       	mov    $0xc,%edx
   9a263:	4c 89 f7             	mov    %r14,%rdi
   9a266:	48 8d 35 f2 a5 f7 ff 	lea    -0x85a0e(%rip),%rsi        # 1485f <__abi_tag+0x14563>
   9a26d:	48 89 d9             	mov    %rbx,%rcx
   9a270:	4d 89 e1             	mov    %r12,%r9
   9a273:	6a 1d                	push   $0x1d
   9a275:	48 8d 05 bd a6 f7 ff 	lea    -0x85943(%rip),%rax        # 14939 <__abi_tag+0x1463d>
   9a27c:	50                   	push   %rax
   9a27d:	e8 7e bb 05 00       	call   f5e00 <_Wfl.fl_vm_bug_at$494622488d4f7894>
   9a282:	48 83 c4 10          	add    $0x10,%rsp
   9a286:	4d 8b 06             	mov    (%r14),%r8
   9a289:	49 8b b6 a4 00 00 00 	mov    0xa4(%r14),%rsi
   9a290:	4d 8b 96 ac 00 00 00 	mov    0xac(%r14),%r10
   9a297:	49 8b 9e b4 00 00 00 	mov    0xb4(%r14),%rbx
   9a29e:	4d 8b be bc 00 00 00 	mov    0xbc(%r14),%r15
   9a2a5:	4d 8b 9e b5 01 00 00 	mov    0x1b5(%r14),%r11
   9a2ac:	41 80 be dd 01 00 00 	cmpb   $0x0,0x1dd(%r14)
   9a2b3:	00 
   9a2b4:	74 65                	je     9a31b <_Wfl.fl_vm_exec$a5081fe92876eb37+0xd1b>
   9a2b6:	49 8b be de 01 00 00 	mov    0x1de(%r14),%rdi
   9a2bd:	48 8b 47 08          	mov    0x8(%rdi),%rax
   9a2c1:	48 3b 47 10          	cmp    0x10(%rdi),%rax
   9a2c5:	73 14                	jae    9a2db <_Wfl.fl_vm_exec$a5081fe92876eb37+0xcdb>
   9a2c7:	48 8b 0f             	mov    (%rdi),%rcx
   9a2ca:	48 8b 54 24 20       	mov    0x20(%rsp),%rdx
   9a2cf:	88 14 01             	mov    %dl,(%rcx,%rax,1)
   9a2d2:	48 ff c0             	inc    %rax
   9a2d5:	48 89 47 08          	mov    %rax,0x8(%rdi)
   9a2d9:	eb 40                	jmp    9a31b <_Wfl.fl_vm_exec$a5081fe92876eb37+0xd1b>
   9a2db:	48 8b 44 24 20       	mov    0x20(%rsp),%rax
   9a2e0:	88 44 24 30          	mov    %al,0x30(%rsp)
   9a2e4:	49 89 f4             	mov    %rsi,%r12
   9a2e7:	48 8d 74 24 30       	lea    0x30(%rsp),%rsi
   9a2ec:	48 89 5c 24 68       	mov    %rbx,0x68(%rsp)
   9a2f1:	4c 89 c3             	mov    %r8,%rbx
   9a2f4:	4c 89 d5             	mov    %r10,%rbp
   9a2f7:	4d 89 ee             	mov    %r13,%r14
   9a2fa:	4d 89 dd             	mov    %r11,%r13
   9a2fd:	e8 fe 3c 0c 00       	call   15e000 <__wolf_rt_list_push>
   9a302:	4d 89 eb             	mov    %r13,%r11
   9a305:	4d 89 f5             	mov    %r14,%r13
   9a308:	4c 8b 74 24 10       	mov    0x10(%rsp),%r14
   9a30d:	49 89 ea             	mov    %rbp,%r10
   9a310:	49 89 d8             	mov    %rbx,%r8
   9a313:	48 8b 5c 24 68       	mov    0x68(%rsp),%rbx
   9a318:	4c 89 e6             	mov    %r12,%rsi
   9a31b:	4c 8b 64 24 20       	mov    0x20(%rsp),%r12
   9a320:	49 83 fc 0a          	cmp    $0xa,%r12
   9a324:	0f 87 e1 02 00 00    	ja     9a60b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x100b>
   9a32a:	48 8d 0d b3 9d f7 ff 	lea    -0x8624d(%rip),%rcx        # 140e4 <__abi_tag+0x13de8>
   9a331:	4a 63 04 a1          	movslq (%rcx,%r12,4),%rax
   9a335:	48 01 c8             	add    %rcx,%rax
   9a338:	ff e0                	jmp    *%rax
   9a33a:	4c 89 f7             	mov    %r14,%rdi
   9a33d:	4c 89 ee             	mov    %r13,%rsi
   9a340:	48 8b 5c 24 08       	mov    0x8(%rsp),%rbx
   9a345:	48 89 da             	mov    %rbx,%rdx
   9a348:	e8 73 70 02 00       	call   c13c0 <_Wfl.fl_read16$7e28ba906e98be2a>
   9a34d:	49 8b 0e             	mov    (%r14),%rcx
   9a350:	48 3b 59 08          	cmp    0x8(%rcx),%rbx
   9a354:	48 8b 6c 24 18       	mov    0x18(%rsp),%rbp
   9a359:	0f 83 84 45 00 00    	jae    9e8e3 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x52e3>
   9a35f:	48 8b 09             	mov    (%rcx),%rcx
   9a362:	48 69 d3 60 01 00 00 	imul   $0x160,%rbx,%rdx
   9a369:	48 8b 4c 11 29       	mov    0x29(%rcx,%rdx,1),%rcx
   9a36e:	48 3b 41 08          	cmp    0x8(%rcx),%rax
   9a372:	0f 83 6b 45 00 00    	jae    9e8e3 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x52e3>
   9a378:	48 8b 09             	mov    (%rcx),%rcx
   9a37b:	48 8d 04 40          	lea    (%rax,%rax,2),%rax
   9a37f:	f3 0f 7e 44 c1 10    	movq   0x10(%rcx,%rax,8),%xmm0
   9a385:	0f 10 0c c1          	movups (%rcx,%rax,8),%xmm1
   9a389:	0f 29 8c 24 40 04 00 	movaps %xmm1,0x440(%rsp)
   9a390:	00 
   9a391:	66 0f d6 84 24 50 04 	movq   %xmm0,0x450(%rsp)
   9a398:	00 00 
   9a39a:	4c 89 f7             	mov    %r14,%rdi
   9a39d:	48 8d b4 24 40 04 00 	lea    0x440(%rsp),%rsi
   9a3a4:	00 
   9a3a5:	e8 36 51 03 00       	call   cf4e0 <_Wfl.fl_push$396d177d8e34f937>
   9a3aa:	e9 76 0c 00 00       	jmp    9b025 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1a25>
   9a3af:	4c 89 f7             	mov    %r14,%rdi
   9a3b2:	4c 89 ee             	mov    %r13,%rsi
   9a3b5:	48 8b 5c 24 08       	mov    0x8(%rsp),%rbx
   9a3ba:	48 89 da             	mov    %rbx,%rdx
   9a3bd:	e8 4e 6f 02 00       	call   c1310 <_Wfl.fl_read8$a2ddcd4dfd790b8d>
   9a3c2:	48 8d 88 00 ff ff ff 	lea    -0x100(%rax),%rcx
   9a3c9:	48 83 f8 7f          	cmp    $0x7f,%rax
   9a3cd:	48 0f 4e c8          	cmovle %rax,%rcx
   9a3d1:	48 c7 84 24 a0 02 00 	movq   $0x2,0x2a0(%rsp)
   9a3d8:	00 02 00 00 00 
   9a3dd:	48 89 8c 24 a8 02 00 	mov    %rcx,0x2a8(%rsp)
   9a3e4:	00 
   9a3e5:	48 c7 84 24 b0 02 00 	movq   $0x0,0x2b0(%rsp)
   9a3ec:	00 00 00 00 00 
   9a3f1:	4c 89 f7             	mov    %r14,%rdi
   9a3f4:	48 8d b4 24 a0 02 00 	lea    0x2a0(%rsp),%rsi
   9a3fb:	00 
   9a3fc:	e8 df 50 03 00       	call   cf4e0 <_Wfl.fl_push$396d177d8e34f937>
   9a401:	48 8b 6c 24 18       	mov    0x18(%rsp),%rbp
   9a406:	e9 1a 0c 00 00       	jmp    9b025 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1a25>
   9a40b:	4c 89 f7             	mov    %r14,%rdi
   9a40e:	4c 89 ee             	mov    %r13,%rsi
   9a411:	48 8b 5c 24 08       	mov    0x8(%rsp),%rbx
   9a416:	48 89 da             	mov    %rbx,%rdx
   9a419:	e8 f2 6e 02 00       	call   c1310 <_Wfl.fl_read8$a2ddcd4dfd790b8d>
   9a41e:	48 03 44 24 40       	add    0x40(%rsp),%rax
   9a423:	48 8b 6c 24 18       	mov    0x18(%rsp),%rbp
   9a428:	0f 80 5a 42 00 00    	jo     9e688 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5088>
   9a42e:	49 8b 8e a4 00 00 00 	mov    0xa4(%r14),%rcx
   9a435:	48 3b 41 08          	cmp    0x8(%rcx),%rax
   9a439:	0f 83 54 45 00 00    	jae    9e993 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5393>
   9a43f:	48 8b 09             	mov    (%rcx),%rcx
   9a442:	48 8d 04 40          	lea    (%rax,%rax,2),%rax
   9a446:	f3 0f 7e 44 c1 10    	movq   0x10(%rcx,%rax,8),%xmm0
   9a44c:	0f 10 0c c1          	movups (%rcx,%rax,8),%xmm1
   9a450:	0f 29 8c 24 00 04 00 	movaps %xmm1,0x400(%rsp)
   9a457:	00 
   9a458:	66 0f d6 84 24 10 04 	movq   %xmm0,0x410(%rsp)
   9a45f:	00 00 
   9a461:	4c 89 f7             	mov    %r14,%rdi
   9a464:	48 8d b4 24 00 04 00 	lea    0x400(%rsp),%rsi
   9a46b:	00 
   9a46c:	e8 6f 50 03 00       	call   cf4e0 <_Wfl.fl_push$396d177d8e34f937>
   9a471:	e9 af 0b 00 00       	jmp    9b025 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1a25>
   9a476:	4c 89 f7             	mov    %r14,%rdi
   9a479:	48 8d b4 24 b8 02 00 	lea    0x2b8(%rsp),%rsi
   9a480:	00 
   9a481:	eb 49                	jmp    9a4cc <_Wfl.fl_vm_exec$a5081fe92876eb37+0xecc>
   9a483:	4c 89 f7             	mov    %r14,%rdi
   9a486:	48 8d b4 24 20 05 00 	lea    0x520(%rsp),%rsi
   9a48d:	00 
   9a48e:	eb 3c                	jmp    9a4cc <_Wfl.fl_vm_exec$a5081fe92876eb37+0xecc>
   9a490:	4c 89 f7             	mov    %r14,%rdi
   9a493:	4c 89 ee             	mov    %r13,%rsi
   9a496:	48 8b 5c 24 08       	mov    0x8(%rsp),%rbx
   9a49b:	48 89 da             	mov    %rbx,%rdx
   9a49e:	e8 6d 6e 02 00       	call   c1310 <_Wfl.fl_read8$a2ddcd4dfd790b8d>
   9a4a3:	49 8b 8e ac 00 00 00 	mov    0xac(%r14),%rcx
   9a4aa:	48 29 c1             	sub    %rax,%rcx
   9a4ad:	48 8b 6c 24 18       	mov    0x18(%rsp),%rbp
   9a4b2:	0f 80 17 42 00 00    	jo     9e6cf <_Wfl.fl_vm_exec$a5081fe92876eb37+0x50cf>
   9a4b8:	49 89 8e ac 00 00 00 	mov    %rcx,0xac(%r14)
   9a4bf:	e9 61 0b 00 00       	jmp    9b025 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1a25>
   9a4c4:	4c 89 f7             	mov    %r14,%rdi
   9a4c7:	48 8d 74 24 50       	lea    0x50(%rsp),%rsi
   9a4cc:	e8 0f 50 03 00       	call   cf4e0 <_Wfl.fl_push$396d177d8e34f937>
   9a4d1:	48 8b 6c 24 18       	mov    0x18(%rsp),%rbp
   9a4d6:	e9 45 0b 00 00       	jmp    9b020 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1a20>
   9a4db:	4c 89 f7             	mov    %r14,%rdi
   9a4de:	4c 89 ee             	mov    %r13,%rsi
   9a4e1:	48 8b 54 24 08       	mov    0x8(%rsp),%rdx
   9a4e6:	e8 25 6e 02 00       	call   c1310 <_Wfl.fl_read8$a2ddcd4dfd790b8d>
   9a4eb:	48 85 c0             	test   %rax,%rax
   9a4ee:	48 8b 6c 24 18       	mov    0x18(%rsp),%rbp
   9a4f3:	0f 8e 27 0b 00 00    	jle    9b020 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1a20>
   9a4f9:	48 89 c3             	mov    %rax,%rbx
   9a4fc:	4c 8d 7c 24 50       	lea    0x50(%rsp),%r15
   9a501:	66 66 66 66 66 66 2e 	data16 data16 data16 data16 data16 cs nopw 0x0(%rax,%rax,1)
   9a508:	0f 1f 84 00 00 00 00 
   9a50f:	00 
   9a510:	4c 89 f7             	mov    %r14,%rdi
   9a513:	4c 89 fe             	mov    %r15,%rsi
   9a516:	e8 c5 4f 03 00       	call   cf4e0 <_Wfl.fl_push$396d177d8e34f937>
   9a51b:	48 ff cb             	dec    %rbx
   9a51e:	75 f0                	jne    9a510 <_Wfl.fl_vm_exec$a5081fe92876eb37+0xf10>
   9a520:	e9 fb 0a 00 00       	jmp    9b020 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1a20>
   9a525:	4c 89 f7             	mov    %r14,%rdi
   9a528:	48 8d b4 24 08 22 00 	lea    0x2208(%rsp),%rsi
   9a52f:	00 
   9a530:	e8 db 40 01 00       	call   ae610 <_Wfl.fl_pop$add7471f880d5ef3>
   9a535:	48 8b 6c 24 18       	mov    0x18(%rsp),%rbp
   9a53a:	e9 e1 0a 00 00       	jmp    9b020 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1a20>
   9a53f:	4c 89 f7             	mov    %r14,%rdi
   9a542:	4c 89 ee             	mov    %r13,%rsi
   9a545:	48 8b 54 24 08       	mov    0x8(%rsp),%rdx
   9a54a:	e8 c1 6d 02 00       	call   c1310 <_Wfl.fl_read8$a2ddcd4dfd790b8d>
   9a54f:	48 89 c3             	mov    %rax,%rbx
   9a552:	4c 89 f7             	mov    %r14,%rdi
   9a555:	48 8d b4 24 88 02 00 	lea    0x288(%rsp),%rsi
   9a55c:	00 
   9a55d:	e8 ae 40 01 00       	call   ae610 <_Wfl.fl_pop$add7471f880d5ef3>
   9a562:	48 03 5c 24 40       	add    0x40(%rsp),%rbx
   9a567:	48 8b 6c 24 18       	mov    0x18(%rsp),%rbp
   9a56c:	0f 80 4a 41 00 00    	jo     9e6bc <_Wfl.fl_vm_exec$a5081fe92876eb37+0x50bc>
   9a572:	49 8b 86 a4 00 00 00 	mov    0xa4(%r14),%rax
   9a579:	48 3b 58 08          	cmp    0x8(%rax),%rbx
   9a57d:	0f 83 47 44 00 00    	jae    9e9ca <_Wfl.fl_vm_exec$a5081fe92876eb37+0x53ca>
   9a583:	48 8b 00             	mov    (%rax),%rax
   9a586:	48 8d 0c 5b          	lea    (%rbx,%rbx,2),%rcx
   9a58a:	48 8b 94 24 88 02 00 	mov    0x288(%rsp),%rdx
   9a591:	00 
   9a592:	48 89 14 c8          	mov    %rdx,(%rax,%rcx,8)
   9a596:	48 8b 94 24 90 02 00 	mov    0x290(%rsp),%rdx
   9a59d:	00 
   9a59e:	48 89 54 c8 08       	mov    %rdx,0x8(%rax,%rcx,8)
   9a5a3:	f3 0f 7e 84 24 98 02 	movq   0x298(%rsp),%xmm0
   9a5aa:	00 00 
   9a5ac:	66 0f d6 44 c8 10    	movq   %xmm0,0x10(%rax,%rcx,8)
   9a5b2:	e9 69 0a 00 00       	jmp    9b020 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1a20>
   9a5b7:	49 ff ca             	dec    %r10
   9a5ba:	48 8b 6c 24 18       	mov    0x18(%rsp),%rbp
   9a5bf:	48 8b 5c 24 08       	mov    0x8(%rsp),%rbx
   9a5c4:	0f 80 d1 40 00 00    	jo     9e69b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x509b>
   9a5ca:	4c 3b 56 08          	cmp    0x8(%rsi),%r10
   9a5ce:	0f 83 d5 43 00 00    	jae    9e9a9 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x53a9>
   9a5d4:	48 8b 06             	mov    (%rsi),%rax
   9a5d7:	4b 8d 0c 52          	lea    (%r10,%r10,2),%rcx
   9a5db:	f3 0f 7e 44 c8 10    	movq   0x10(%rax,%rcx,8),%xmm0
   9a5e1:	0f 10 0c c8          	movups (%rax,%rcx,8),%xmm1
   9a5e5:	0f 29 8c 24 20 04 00 	movaps %xmm1,0x420(%rsp)
   9a5ec:	00 
   9a5ed:	66 0f d6 84 24 30 04 	movq   %xmm0,0x430(%rsp)
   9a5f4:	00 00 
   9a5f6:	4c 89 f7             	mov    %r14,%rdi
   9a5f9:	48 8d b4 24 20 04 00 	lea    0x420(%rsp),%rsi
   9a600:	00 
   9a601:	e8 da 4e 03 00       	call   cf4e0 <_Wfl.fl_push$396d177d8e34f937>
   9a606:	e9 1a 0a 00 00       	jmp    9b025 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1a25>
   9a60b:	49 8d 44 24 f5       	lea    -0xb(%r12),%rax
   9a610:	48 83 f8 01          	cmp    $0x1,%rax
   9a614:	0f 87 ea 00 00 00    	ja     9a704 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1104>
   9a61a:	4c 89 f7             	mov    %r14,%rdi
   9a61d:	4c 89 ee             	mov    %r13,%rsi
   9a620:	48 8b 54 24 08       	mov    0x8(%rsp),%rdx
   9a625:	e8 e6 6c 02 00       	call   c1310 <_Wfl.fl_read8$a2ddcd4dfd790b8d>
   9a62a:	49 8b 0e             	mov    (%r14),%rcx
   9a62d:	48 8b 51 08          	mov    0x8(%rcx),%rdx
   9a631:	48 8b 74 24 48       	mov    0x48(%rsp),%rsi
   9a636:	48 39 d6             	cmp    %rdx,%rsi
   9a639:	0f 83 3e 43 00 00    	jae    9e97d <_Wfl.fl_vm_exec$a5081fe92876eb37+0x537d>
   9a63f:	48 8b 09             	mov    (%rcx),%rcx
   9a642:	48 69 f6 60 01 00 00 	imul   $0x160,%rsi,%rsi
   9a649:	48 8b 74 31 29       	mov    0x29(%rcx,%rsi,1),%rsi
   9a64e:	48 3b 46 08          	cmp    0x8(%rsi),%rax
   9a652:	48 8b 6c 24 18       	mov    0x18(%rsp),%rbp
   9a657:	0f 83 20 43 00 00    	jae    9e97d <_Wfl.fl_vm_exec$a5081fe92876eb37+0x537d>
   9a65d:	48 8b 3e             	mov    (%rsi),%rdi
   9a660:	48 8d 04 40          	lea    (%rax,%rax,2),%rax
   9a664:	48 8b 34 c7          	mov    (%rdi,%rax,8),%rsi
   9a668:	4c 8d 46 f4          	lea    -0xc(%rsi),%r8
   9a66c:	49 83 f8 f8          	cmp    $0xfffffffffffffff8,%r8
   9a670:	0f 82 83 42 00 00    	jb     9e8f9 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x52f9>
   9a676:	48 8b 44 c7 08       	mov    0x8(%rdi,%rax,8),%rax
   9a67b:	48 85 c0             	test   %rax,%rax
   9a67e:	0f 88 75 42 00 00    	js     9e8f9 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x52f9>
   9a684:	41 89 c7             	mov    %eax,%r15d
   9a687:	49 39 d7             	cmp    %rdx,%r15
   9a68a:	0f 8d 69 42 00 00    	jge    9e8f9 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x52f9>
   9a690:	49 69 df 60 01 00 00 	imul   $0x160,%r15,%rbx
   9a697:	48 39 34 19          	cmp    %rsi,(%rcx,%rbx,1)
   9a69b:	0f 85 58 42 00 00    	jne    9e8f9 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x52f9>
   9a6a1:	48 01 d9             	add    %rbx,%rcx
   9a6a4:	48 c1 e8 20          	shr    $0x20,%rax
   9a6a8:	48 39 41 19          	cmp    %rax,0x19(%rcx)
   9a6ac:	0f 85 47 42 00 00    	jne    9e8f9 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x52f9>
   9a6b2:	4c 8b a1 29 01 00 00 	mov    0x129(%rcx),%r12
   9a6b9:	48 83 7c 24 20 0b    	cmpq   $0xb,0x20(%rsp)
   9a6bf:	0f 85 fb 07 00 00    	jne    9aec0 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x18c0>
   9a6c5:	4d 85 e4             	test   %r12,%r12
   9a6c8:	0f 88 b9 08 00 00    	js     9af87 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1987>
   9a6ce:	49 8b 86 a4 00 00 00 	mov    0xa4(%r14),%rax
   9a6d5:	4c 3b 60 08          	cmp    0x8(%rax),%r12
   9a6d9:	48 8b 5c 24 08       	mov    0x8(%rsp),%rbx
   9a6de:	0f 83 5f 43 00 00    	jae    9ea43 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5443>
   9a6e4:	48 8b 10             	mov    (%rax),%rdx
   9a6e7:	4b 8d 34 64          	lea    (%r12,%r12,2),%rsi
   9a6eb:	48 8d 0c f2          	lea    (%rdx,%rsi,8),%rcx
   9a6ef:	48 8d 04 f2          	lea    (%rdx,%rsi,8),%rax
   9a6f3:	48 83 c0 08          	add    $0x8,%rax
   9a6f7:	48 8d 14 f2          	lea    (%rdx,%rsi,8),%rdx
   9a6fb:	48 83 c2 10          	add    $0x10,%rdx
   9a6ff:	e9 9d 08 00 00       	jmp    9afa1 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x19a1>
   9a704:	49 83 fc 0d          	cmp    $0xd,%r12
   9a708:	75 3a                	jne    9a744 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1144>
   9a70a:	4c 89 f7             	mov    %r14,%rdi
   9a70d:	4c 89 ee             	mov    %r13,%rsi
   9a710:	48 8b 54 24 08       	mov    0x8(%rsp),%rdx
   9a715:	e8 f6 6b 02 00       	call   c1310 <_Wfl.fl_read8$a2ddcd4dfd790b8d>
   9a71a:	48 89 c3             	mov    %rax,%rbx
   9a71d:	48 03 5c 24 40       	add    0x40(%rsp),%rbx
   9a722:	0f 80 c8 3f 00 00    	jo     9e6f0 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x50f0>
   9a728:	4c 89 f7             	mov    %r14,%rdi
   9a72b:	48 89 de             	mov    %rbx,%rsi
   9a72e:	e8 4d 4e 03 00       	call   cf580 <_Wfl.fl_close_upvals$345768e8f0248c70>
   9a733:	49 89 9e ac 00 00 00 	mov    %rbx,0xac(%r14)
   9a73a:	48 8b 6c 24 18       	mov    0x18(%rsp),%rbp
   9a73f:	e9 dc 08 00 00       	jmp    9b020 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1a20>
   9a744:	49 8d 44 24 f2       	lea    -0xe(%r12),%rax
   9a749:	48 83 f8 02          	cmp    $0x2,%rax
   9a74d:	0f 87 cb 07 00 00    	ja     9af1e <_Wfl.fl_vm_exec$a5081fe92876eb37+0x191e>
   9a753:	4c 89 f7             	mov    %r14,%rdi
   9a756:	4c 89 ee             	mov    %r13,%rsi
   9a759:	48 8b 5c 24 08       	mov    0x8(%rsp),%rbx
   9a75e:	48 89 da             	mov    %rbx,%rdx
   9a761:	e8 5a 6c 02 00       	call   c13c0 <_Wfl.fl_read16$7e28ba906e98be2a>
   9a766:	4d 8b 46 20          	mov    0x20(%r14),%r8
   9a76a:	49 8b 76 48          	mov    0x48(%r14),%rsi
   9a76e:	41 0f b7 7e 50       	movzwl 0x50(%r14),%edi
   9a773:	49 8b 56 62          	mov    0x62(%r14),%rdx
   9a777:	49 8b 8e 8a 00 00 00 	mov    0x8a(%r14),%rcx
   9a77e:	41 0f 10 06          	movups (%r14),%xmm0
   9a782:	0f 29 84 24 10 06 00 	movaps %xmm0,0x610(%rsp)
   9a789:	00 
   9a78a:	41 0f 10 46 10       	movups 0x10(%r14),%xmm0
   9a78f:	0f 29 84 24 20 06 00 	movaps %xmm0,0x620(%rsp)
   9a796:	00 
   9a797:	4c 89 84 24 30 06 00 	mov    %r8,0x630(%rsp)
   9a79e:	00 
   9a79f:	45 0f b7 86 a2 00 00 	movzwl 0xa2(%r14),%r8d
   9a7a6:	00 
   9a7a7:	41 0f 10 46 28       	movups 0x28(%r14),%xmm0
   9a7ac:	0f 11 84 24 38 06 00 	movups %xmm0,0x638(%rsp)
   9a7b3:	00 
   9a7b4:	41 0f 10 46 38       	movups 0x38(%r14),%xmm0
   9a7b9:	0f 11 84 24 48 06 00 	movups %xmm0,0x648(%rsp)
   9a7c0:	00 
   9a7c1:	41 0f 10 46 52       	movups 0x52(%r14),%xmm0
   9a7c6:	48 89 b4 24 58 06 00 	mov    %rsi,0x658(%rsp)
   9a7cd:	00 
   9a7ce:	48 89 bc 24 60 06 00 	mov    %rdi,0x660(%rsp)
   9a7d5:	00 
   9a7d6:	0f 11 84 24 68 06 00 	movups %xmm0,0x668(%rsp)
   9a7dd:	00 
   9a7de:	48 89 94 24 78 06 00 	mov    %rdx,0x678(%rsp)
   9a7e5:	00 
   9a7e6:	41 0f 10 46 6a       	movups 0x6a(%r14),%xmm0
   9a7eb:	0f 29 84 24 80 06 00 	movaps %xmm0,0x680(%rsp)
   9a7f2:	00 
   9a7f3:	41 0f 10 46 7a       	movups 0x7a(%r14),%xmm0
   9a7f8:	0f 29 84 24 90 06 00 	movaps %xmm0,0x690(%rsp)
   9a7ff:	00 
   9a800:	41 0f 10 86 92 00 00 	movups 0x92(%r14),%xmm0
   9a807:	00 
   9a808:	48 89 8c 24 a0 06 00 	mov    %rcx,0x6a0(%rsp)
   9a80f:	00 
   9a810:	0f 11 84 24 a8 06 00 	movups %xmm0,0x6a8(%rsp)
   9a817:	00 
   9a818:	41 0f 10 86 a4 00 00 	movups 0xa4(%r14),%xmm0
   9a81f:	00 
   9a820:	49 8b 96 c4 00 00 00 	mov    0xc4(%r14),%rdx
   9a827:	41 0f b6 8e 5c 01 00 	movzbl 0x15c(%r14),%ecx
   9a82e:	00 
   9a82f:	4c 89 84 24 b8 06 00 	mov    %r8,0x6b8(%rsp)
   9a836:	00 
   9a837:	0f 29 84 24 c0 06 00 	movaps %xmm0,0x6c0(%rsp)
   9a83e:	00 
   9a83f:	41 0f 10 86 b4 00 00 	movups 0xb4(%r14),%xmm0
   9a846:	00 
   9a847:	0f 29 84 24 d0 06 00 	movaps %xmm0,0x6d0(%rsp)
   9a84e:	00 
   9a84f:	41 0f 10 86 cc 00 00 	movups 0xcc(%r14),%xmm0
   9a856:	00 
   9a857:	48 89 94 24 e0 06 00 	mov    %rdx,0x6e0(%rsp)
   9a85e:	00 
   9a85f:	0f 11 84 24 e8 06 00 	movups %xmm0,0x6e8(%rsp)
   9a866:	00 
   9a867:	41 0f 10 86 dc 00 00 	movups 0xdc(%r14),%xmm0
   9a86e:	00 
   9a86f:	0f 11 84 24 f8 06 00 	movups %xmm0,0x6f8(%rsp)
   9a876:	00 
   9a877:	41 0f 10 86 ec 00 00 	movups 0xec(%r14),%xmm0
   9a87e:	00 
   9a87f:	0f 11 84 24 08 07 00 	movups %xmm0,0x708(%rsp)
   9a886:	00 
   9a887:	41 0f 10 86 fc 00 00 	movups 0xfc(%r14),%xmm0
   9a88e:	00 
   9a88f:	0f 11 84 24 18 07 00 	movups %xmm0,0x718(%rsp)
   9a896:	00 
   9a897:	41 0f 10 86 0c 01 00 	movups 0x10c(%r14),%xmm0
   9a89e:	00 
   9a89f:	0f 11 84 24 28 07 00 	movups %xmm0,0x728(%rsp)
   9a8a6:	00 
   9a8a7:	41 0f 10 86 1c 01 00 	movups 0x11c(%r14),%xmm0
   9a8ae:	00 
   9a8af:	0f 11 84 24 38 07 00 	movups %xmm0,0x738(%rsp)
   9a8b6:	00 
   9a8b7:	41 0f 10 86 2c 01 00 	movups 0x12c(%r14),%xmm0
   9a8be:	00 
   9a8bf:	0f 11 84 24 48 07 00 	movups %xmm0,0x748(%rsp)
   9a8c6:	00 
   9a8c7:	41 0f 10 86 3c 01 00 	movups 0x13c(%r14),%xmm0
   9a8ce:	00 
   9a8cf:	0f 11 84 24 58 07 00 	movups %xmm0,0x758(%rsp)
   9a8d6:	00 
   9a8d7:	41 0f 10 86 4c 01 00 	movups 0x14c(%r14),%xmm0
   9a8de:	00 
   9a8df:	0f 11 84 24 68 07 00 	movups %xmm0,0x768(%rsp)
   9a8e6:	00 
   9a8e7:	41 0f 10 86 5d 01 00 	movups 0x15d(%r14),%xmm0
   9a8ee:	00 
   9a8ef:	88 8c 24 78 07 00 00 	mov    %cl,0x778(%rsp)
   9a8f6:	0f 29 84 24 80 07 00 	movaps %xmm0,0x780(%rsp)
   9a8fd:	00 
   9a8fe:	41 0f 10 86 6d 01 00 	movups 0x16d(%r14),%xmm0
   9a905:	00 
   9a906:	41 0f b6 96 dd 01 00 	movzbl 0x1dd(%r14),%edx
   9a90d:	00 
   9a90e:	49 8b b6 ee 01 00 00 	mov    0x1ee(%r14),%rsi
   9a915:	49 8b be 16 02 00 00 	mov    0x216(%r14),%rdi
   9a91c:	45 0f b6 86 1e 02 00 	movzbl 0x21e(%r14),%r8d
   9a923:	00 
   9a924:	49 8b 8e 2f 02 00 00 	mov    0x22f(%r14),%rcx
   9a92b:	0f 29 84 24 90 07 00 	movaps %xmm0,0x790(%rsp)
   9a932:	00 
   9a933:	41 0f 10 86 7d 01 00 	movups 0x17d(%r14),%xmm0
   9a93a:	00 
   9a93b:	0f 29 84 24 a0 07 00 	movaps %xmm0,0x7a0(%rsp)
   9a942:	00 
   9a943:	41 0f 10 86 8d 01 00 	movups 0x18d(%r14),%xmm0
   9a94a:	00 
   9a94b:	0f 29 84 24 b0 07 00 	movaps %xmm0,0x7b0(%rsp)
   9a952:	00 
   9a953:	41 0f 10 86 9d 01 00 	movups 0x19d(%r14),%xmm0
   9a95a:	00 
   9a95b:	0f 29 84 24 c0 07 00 	movaps %xmm0,0x7c0(%rsp)
   9a962:	00 
   9a963:	41 0f 10 86 ad 01 00 	movups 0x1ad(%r14),%xmm0
   9a96a:	00 
   9a96b:	0f 29 84 24 d0 07 00 	movaps %xmm0,0x7d0(%rsp)
   9a972:	00 
   9a973:	41 0f 10 86 bd 01 00 	movups 0x1bd(%r14),%xmm0
   9a97a:	00 
   9a97b:	0f 29 84 24 e0 07 00 	movaps %xmm0,0x7e0(%rsp)
   9a982:	00 
   9a983:	41 0f 10 86 cd 01 00 	movups 0x1cd(%r14),%xmm0
   9a98a:	00 
   9a98b:	0f 29 84 24 f0 07 00 	movaps %xmm0,0x7f0(%rsp)
   9a992:	00 
   9a993:	88 94 24 00 08 00 00 	mov    %dl,0x800(%rsp)
   9a99a:	49 8b 96 47 02 00 00 	mov    0x247(%r14),%rdx
   9a9a1:	41 0f 10 86 de 01 00 	movups 0x1de(%r14),%xmm0
   9a9a8:	00 
   9a9a9:	0f 11 84 24 08 08 00 	movups %xmm0,0x808(%rsp)
   9a9b0:	00 
   9a9b1:	48 89 b4 24 18 08 00 	mov    %rsi,0x818(%rsp)
   9a9b8:	00 
   9a9b9:	49 8b b6 7f 02 00 00 	mov    0x27f(%r14),%rsi
   9a9c0:	41 0f 10 86 f6 01 00 	movups 0x1f6(%r14),%xmm0
   9a9c7:	00 
   9a9c8:	0f 29 84 24 20 08 00 	movaps %xmm0,0x820(%rsp)
   9a9cf:	00 
   9a9d0:	41 0f 10 86 06 02 00 	movups 0x206(%r14),%xmm0
   9a9d7:	00 
   9a9d8:	0f 29 84 24 30 08 00 	movaps %xmm0,0x830(%rsp)
   9a9df:	00 
   9a9e0:	41 0f 10 86 1f 02 00 	movups 0x21f(%r14),%xmm0
   9a9e7:	00 
   9a9e8:	48 89 bc 24 40 08 00 	mov    %rdi,0x840(%rsp)
   9a9ef:	00 
   9a9f0:	4c 89 84 24 48 08 00 	mov    %r8,0x848(%rsp)
   9a9f7:	00 
   9a9f8:	0f 29 84 24 50 08 00 	movaps %xmm0,0x850(%rsp)
   9a9ff:	00 
   9aa00:	41 0f 10 86 37 02 00 	movups 0x237(%r14),%xmm0
   9aa07:	00 
   9aa08:	48 89 8c 24 60 08 00 	mov    %rcx,0x860(%rsp)
   9aa0f:	00 
   9aa10:	0f 11 84 24 68 08 00 	movups %xmm0,0x868(%rsp)
   9aa17:	00 
   9aa18:	41 0f 10 86 4f 02 00 	movups 0x24f(%r14),%xmm0
   9aa1f:	00 
   9aa20:	48 89 94 24 78 08 00 	mov    %rdx,0x878(%rsp)
   9aa27:	00 
   9aa28:	0f 29 84 24 80 08 00 	movaps %xmm0,0x880(%rsp)
   9aa2f:	00 
   9aa30:	41 0f 10 86 5f 02 00 	movups 0x25f(%r14),%xmm0
   9aa37:	00 
   9aa38:	0f 29 84 24 90 08 00 	movaps %xmm0,0x890(%rsp)
   9aa3f:	00 
   9aa40:	41 0f 10 86 6f 02 00 	movups 0x26f(%r14),%xmm0
   9aa47:	00 
   9aa48:	0f 29 84 24 a0 08 00 	movaps %xmm0,0x8a0(%rsp)
   9aa4f:	00 
   9aa50:	41 0f 10 86 87 02 00 	movups 0x287(%r14),%xmm0
   9aa57:	00 
   9aa58:	48 89 b4 24 b0 08 00 	mov    %rsi,0x8b0(%rsp)
   9aa5f:	00 
   9aa60:	0f 11 84 24 b8 08 00 	movups %xmm0,0x8b8(%rsp)
   9aa67:	00 
   9aa68:	41 0f 10 86 97 02 00 	movups 0x297(%r14),%xmm0
   9aa6f:	00 
   9aa70:	0f 11 84 24 c8 08 00 	movups %xmm0,0x8c8(%rsp)
   9aa77:	00 
   9aa78:	41 0f 10 86 a7 02 00 	movups 0x2a7(%r14),%xmm0
   9aa7f:	00 
   9aa80:	0f 11 84 24 d8 08 00 	movups %xmm0,0x8d8(%rsp)
   9aa87:	00 
   9aa88:	41 0f 10 86 b7 02 00 	movups 0x2b7(%r14),%xmm0
   9aa8f:	00 
   9aa90:	0f 11 84 24 e8 08 00 	movups %xmm0,0x8e8(%rsp)
   9aa97:	00 
   9aa98:	41 0f 10 86 c7 02 00 	movups 0x2c7(%r14),%xmm0
   9aa9f:	00 
   9aaa0:	0f 11 84 24 f8 08 00 	movups %xmm0,0x8f8(%rsp)
   9aaa7:	00 
   9aaa8:	41 0f 10 86 d7 02 00 	movups 0x2d7(%r14),%xmm0
   9aaaf:	00 
   9aab0:	0f 11 84 24 08 09 00 	movups %xmm0,0x908(%rsp)
   9aab7:	00 
   9aab8:	41 0f 10 86 e7 02 00 	movups 0x2e7(%r14),%xmm0
   9aabf:	00 
   9aac0:	0f 11 84 24 18 09 00 	movups %xmm0,0x918(%rsp)
   9aac7:	00 
   9aac8:	41 0f 10 86 f7 02 00 	movups 0x2f7(%r14),%xmm0
   9aacf:	00 
   9aad0:	0f 11 84 24 28 09 00 	movups %xmm0,0x928(%rsp)
   9aad7:	00 
   9aad8:	41 0f 10 86 07 03 00 	movups 0x307(%r14),%xmm0
   9aadf:	00 
   9aae0:	49 8b 8e 17 03 00 00 	mov    0x317(%r14),%rcx
   9aae7:	49 8b 96 3f 03 00 00 	mov    0x33f(%r14),%rdx
   9aaee:	49 8b b6 57 03 00 00 	mov    0x357(%r14),%rsi
   9aaf5:	41 0f b6 be 5f 03 00 	movzbl 0x35f(%r14),%edi
   9aafc:	00 
   9aafd:	4d 8b 86 90 03 00 00 	mov    0x390(%r14),%r8
   9ab04:	0f 11 84 24 38 09 00 	movups %xmm0,0x938(%rsp)
   9ab0b:	00 
   9ab0c:	48 89 8c 24 48 09 00 	mov    %rcx,0x948(%rsp)
   9ab13:	00 
   9ab14:	41 0f b6 8e a8 03 00 	movzbl 0x3a8(%r14),%ecx
   9ab1b:	00 
   9ab1c:	41 0f 10 86 1f 03 00 	movups 0x31f(%r14),%xmm0
   9ab23:	00 
   9ab24:	0f 29 84 24 50 09 00 	movaps %xmm0,0x950(%rsp)
   9ab2b:	00 
   9ab2c:	41 0f 10 86 2f 03 00 	movups 0x32f(%r14),%xmm0
   9ab33:	00 
   9ab34:	0f 29 84 24 60 09 00 	movaps %xmm0,0x960(%rsp)
   9ab3b:	00 
   9ab3c:	48 89 94 24 70 09 00 	mov    %rdx,0x970(%rsp)
   9ab43:	00 
   9ab44:	49 8b 96 b9 03 00 00 	mov    0x3b9(%r14),%rdx
   9ab4b:	41 0f 10 86 47 03 00 	movups 0x347(%r14),%xmm0
   9ab52:	00 
   9ab53:	0f 11 84 24 78 09 00 	movups %xmm0,0x978(%rsp)
   9ab5a:	00 
   9ab5b:	48 89 b4 24 88 09 00 	mov    %rsi,0x988(%rsp)
   9ab62:	00 
   9ab63:	49 8b b6 01 04 00 00 	mov    0x401(%r14),%rsi
   9ab6a:	41 0f 10 86 60 03 00 	movups 0x360(%r14),%xmm0
   9ab71:	00 
   9ab72:	48 89 bc 24 90 09 00 	mov    %rdi,0x990(%rsp)
   9ab79:	00 
   9ab7a:	0f 11 84 24 98 09 00 	movups %xmm0,0x998(%rsp)
   9ab81:	00 
   9ab82:	41 0f 10 86 70 03 00 	movups 0x370(%r14),%xmm0
   9ab89:	00 
   9ab8a:	0f 11 84 24 a8 09 00 	movups %xmm0,0x9a8(%rsp)
   9ab91:	00 
   9ab92:	41 0f 10 86 80 03 00 	movups 0x380(%r14),%xmm0
   9ab99:	00 
   9ab9a:	0f 11 84 24 b8 09 00 	movups %xmm0,0x9b8(%rsp)
   9aba1:	00 
   9aba2:	41 0f 10 86 98 03 00 	movups 0x398(%r14),%xmm0
   9aba9:	00 
   9abaa:	4c 89 84 24 c8 09 00 	mov    %r8,0x9c8(%rsp)
   9abb1:	00 
   9abb2:	0f 29 84 24 d0 09 00 	movaps %xmm0,0x9d0(%rsp)
   9abb9:	00 
   9abba:	41 0f 10 86 a9 03 00 	movups 0x3a9(%r14),%xmm0
   9abc1:	00 
   9abc2:	48 89 8c 24 e0 09 00 	mov    %rcx,0x9e0(%rsp)
   9abc9:	00 
   9abca:	0f 11 84 24 e8 09 00 	movups %xmm0,0x9e8(%rsp)
   9abd1:	00 
   9abd2:	41 0f 10 86 c1 03 00 	movups 0x3c1(%r14),%xmm0
   9abd9:	00 
   9abda:	48 89 94 24 f8 09 00 	mov    %rdx,0x9f8(%rsp)
   9abe1:	00 
   9abe2:	0f 29 84 24 00 0a 00 	movaps %xmm0,0xa00(%rsp)
   9abe9:	00 
   9abea:	41 0f 10 86 d1 03 00 	movups 0x3d1(%r14),%xmm0
   9abf1:	00 
   9abf2:	0f 29 84 24 10 0a 00 	movaps %xmm0,0xa10(%rsp)
   9abf9:	00 
   9abfa:	41 0f 10 86 e1 03 00 	movups 0x3e1(%r14),%xmm0
   9ac01:	00 
   9ac02:	0f 29 84 24 20 0a 00 	movaps %xmm0,0xa20(%rsp)
   9ac09:	00 
   9ac0a:	41 0f 10 86 f1 03 00 	movups 0x3f1(%r14),%xmm0
   9ac11:	00 
   9ac12:	0f 29 84 24 30 0a 00 	movaps %xmm0,0xa30(%rsp)
   9ac19:	00 
   9ac1a:	41 0f 10 86 09 04 00 	movups 0x409(%r14),%xmm0
   9ac21:	00 
   9ac22:	48 89 b4 24 40 0a 00 	mov    %rsi,0xa40(%rsp)
   9ac29:	00 
   9ac2a:	0f 11 84 24 48 0a 00 	movups %xmm0,0xa48(%rsp)
   9ac31:	00 
   9ac32:	41 0f 10 86 19 04 00 	movups 0x419(%r14),%xmm0
   9ac39:	00 
   9ac3a:	49 8b 8e 49 04 00 00 	mov    0x449(%r14),%rcx
   9ac41:	49 8b 96 61 04 00 00 	mov    0x461(%r14),%rdx
   9ac48:	41 0f b6 b6 69 04 00 	movzbl 0x469(%r14),%esi
   9ac4f:	00 
   9ac50:	41 0f b7 be 6a 04 00 	movzwl 0x46a(%r14),%edi
   9ac57:	00 
   9ac58:	c1 e7 08             	shl    $0x8,%edi
   9ac5b:	48 09 f7             	or     %rsi,%rdi
   9ac5e:	0f 11 84 24 58 0a 00 	movups %xmm0,0xa58(%rsp)
   9ac65:	00 
   9ac66:	41 0f 10 86 29 04 00 	movups 0x429(%r14),%xmm0
   9ac6d:	00 
   9ac6e:	0f 11 84 24 68 0a 00 	movups %xmm0,0xa68(%rsp)
   9ac75:	00 
   9ac76:	41 0f 10 86 39 04 00 	movups 0x439(%r14),%xmm0
   9ac7d:	00 
   9ac7e:	0f 11 84 24 78 0a 00 	movups %xmm0,0xa78(%rsp)
   9ac85:	00 
   9ac86:	41 0f 10 86 51 04 00 	movups 0x451(%r14),%xmm0
   9ac8d:	00 
   9ac8e:	48 89 8c 24 88 0a 00 	mov    %rcx,0xa88(%rsp)
   9ac95:	00 
   9ac96:	0f 29 84 24 90 0a 00 	movaps %xmm0,0xa90(%rsp)
   9ac9d:	00 
   9ac9e:	41 0f 10 86 6c 04 00 	movups 0x46c(%r14),%xmm0
   9aca5:	00 
   9aca6:	48 89 94 24 a0 0a 00 	mov    %rdx,0xaa0(%rsp)
   9acad:	00 
   9acae:	48 89 bc 24 a8 0a 00 	mov    %rdi,0xaa8(%rsp)
   9acb5:	00 
   9acb6:	0f 29 84 24 b0 0a 00 	movaps %xmm0,0xab0(%rsp)
   9acbd:	00 
   9acbe:	41 0f 10 86 7c 04 00 	movups 0x47c(%r14),%xmm0
   9acc5:	00 
   9acc6:	41 0f b7 8e 8c 04 00 	movzwl 0x48c(%r14),%ecx
   9accd:	00 
   9acce:	0f 29 84 24 c0 0a 00 	movaps %xmm0,0xac0(%rsp)
   9acd5:	00 
   9acd6:	41 0f 10 86 8e 04 00 	movups 0x48e(%r14),%xmm0
   9acdd:	00 
   9acde:	48 89 8c 24 d0 0a 00 	mov    %rcx,0xad0(%rsp)
   9ace5:	00 
   9ace6:	0f 11 84 24 d8 0a 00 	movups %xmm0,0xad8(%rsp)
   9aced:	00 
   9acee:	41 0f 10 86 9e 04 00 	movups 0x49e(%r14),%xmm0
   9acf5:	00 
   9acf6:	41 0f b6 8e ae 04 00 	movzbl 0x4ae(%r14),%ecx
   9acfd:	00 
   9acfe:	0f 11 84 24 e8 0a 00 	movups %xmm0,0xae8(%rsp)
   9ad05:	00 
   9ad06:	41 0f 10 86 af 04 00 	movups 0x4af(%r14),%xmm0
   9ad0d:	00 
   9ad0e:	88 8c 24 f8 0a 00 00 	mov    %cl,0xaf8(%rsp)
   9ad15:	0f 29 84 24 00 0b 00 	movaps %xmm0,0xb00(%rsp)
   9ad1c:	00 
   9ad1d:	41 0f 10 86 bf 04 00 	movups 0x4bf(%r14),%xmm0
   9ad24:	00 
   9ad25:	0f 29 84 24 10 0b 00 	movaps %xmm0,0xb10(%rsp)
   9ad2c:	00 
   9ad2d:	f3 41 0f 6f 86 cf 04 	movdqu 0x4cf(%r14),%xmm0
   9ad34:	00 00 
   9ad36:	66 0f 7f 84 24 20 0b 	movdqa %xmm0,0xb20(%rsp)
   9ad3d:	00 00 
   9ad3f:	49 8b 2e             	mov    (%r14),%rbp
   9ad42:	49 8b 8e df 04 00 00 	mov    0x4df(%r14),%rcx
   9ad49:	4d 8b 8e 14 01 00 00 	mov    0x114(%r14),%r9
   9ad50:	4d 8b a6 1c 01 00 00 	mov    0x11c(%r14),%r12
   9ad57:	4d 8b 7e 6a          	mov    0x6a(%r14),%r15
   9ad5b:	48 89 8c 24 30 0b 00 	mov    %rcx,0xb30(%rsp)
   9ad62:	00 
   9ad63:	48 8b 4d 08          	mov    0x8(%rbp),%rcx
   9ad67:	48 39 cb             	cmp    %rcx,%rbx
   9ad6a:	0f 83 a7 3c 00 00    	jae    9ea17 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5417>
   9ad70:	48 8b 55 00          	mov    0x0(%rbp),%rdx
   9ad74:	48 69 f3 60 01 00 00 	imul   $0x160,%rbx,%rsi
   9ad7b:	48 8b 74 32 29       	mov    0x29(%rdx,%rsi,1),%rsi
   9ad80:	48 3b 46 08          	cmp    0x8(%rsi),%rax
   9ad84:	48 8b 7c 24 48       	mov    0x48(%rsp),%rdi
   9ad89:	0f 83 88 3c 00 00    	jae    9ea17 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5417>
   9ad8f:	48 8b 36             	mov    (%rsi),%rsi
   9ad92:	48 8d 04 40          	lea    (%rax,%rax,2),%rax
   9ad96:	4c 8b 2c c6          	mov    (%rsi,%rax,8),%r13
   9ad9a:	4c 8b 44 c6 08       	mov    0x8(%rsi,%rax,8),%r8
   9ad9f:	f3 0f 7e 44 c6 10    	movq   0x10(%rsi,%rax,8),%xmm0
   9ada5:	4c 89 ac 24 88 00 00 	mov    %r13,0x88(%rsp)
   9adac:	00 
   9adad:	4c 89 84 24 90 00 00 	mov    %r8,0x90(%rsp)
   9adb4:	00 
   9adb5:	66 0f d6 84 24 98 00 	movq   %xmm0,0x98(%rsp)
   9adbc:	00 00 
   9adbe:	48 39 cf             	cmp    %rcx,%rdi
   9adc1:	0f 83 e6 37 00 00    	jae    9e5ad <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4fad>
   9adc7:	4c 89 4c 24 68       	mov    %r9,0x68(%rsp)
   9adcc:	4c 89 44 24 38       	mov    %r8,0x38(%rsp)
   9add1:	48 69 f7 60 01 00 00 	imul   $0x160,%rdi,%rsi
   9add8:	48 8b 8c 32 e1 00 00 	mov    0xe1(%rdx,%rsi,1),%rcx
   9addf:	00 
   9ade0:	48 8b 84 32 e9 00 00 	mov    0xe9(%rdx,%rsi,1),%rax
   9ade7:	00 
   9ade8:	f3 0f 7e 84 32 f1 00 	movq   0xf1(%rdx,%rsi,1),%xmm0
   9adef:	00 00 
   9adf1:	48 89 8c 24 50 01 00 	mov    %rcx,0x150(%rsp)
   9adf8:	00 
   9adf9:	48 89 84 24 58 01 00 	mov    %rax,0x158(%rsp)
   9ae00:	00 
   9ae01:	66 0f d6 84 24 60 01 	movq   %xmm0,0x160(%rsp)
   9ae08:	00 00 
   9ae0a:	48 83 7c 24 20 0f    	cmpq   $0xf,0x20(%rsp)
   9ae10:	0f 84 59 21 00 00    	je     9cf6f <_Wfl.fl_vm_exec$a5081fe92876eb37+0x396f>
   9ae16:	48 83 7c 24 20 0e    	cmpq   $0xe,0x20(%rsp)
   9ae1c:	0f 85 7a 29 00 00    	jne    9d79c <_Wfl.fl_vm_exec$a5081fe92876eb37+0x419c>
   9ae22:	49 83 fd 04          	cmp    $0x4,%r13
   9ae26:	0f 87 94 36 00 00    	ja     9e4c0 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4ec0>
   9ae2c:	ba 16 00 00 00       	mov    $0x16,%edx
   9ae31:	4c 0f a3 ea          	bt     %r13,%rdx
   9ae35:	0f 83 85 36 00 00    	jae    9e4c0 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4ec0>
   9ae3b:	48 8d 51 f4          	lea    -0xc(%rcx),%rdx
   9ae3f:	48 83 fa f8          	cmp    $0xfffffffffffffff8,%rdx
   9ae43:	0f 82 8b 3f 00 00    	jb     9edd4 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x57d4>
   9ae49:	48 85 c0             	test   %rax,%rax
   9ae4c:	0f 88 82 3f 00 00    	js     9edd4 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x57d4>
   9ae52:	89 c3                	mov    %eax,%ebx
   9ae54:	48 3b 5d 08          	cmp    0x8(%rbp),%rbx
   9ae58:	0f 8d 76 3f 00 00    	jge    9edd4 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x57d4>
   9ae5e:	48 8b 55 00          	mov    0x0(%rbp),%rdx
   9ae62:	4c 69 fb 60 01 00 00 	imul   $0x160,%rbx,%r15
   9ae69:	4a 39 0c 3a          	cmp    %rcx,(%rdx,%r15,1)
   9ae6d:	0f 85 61 3f 00 00    	jne    9edd4 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x57d4>
   9ae73:	48 c1 e8 20          	shr    $0x20,%rax
   9ae77:	4a 39 44 3a 19       	cmp    %rax,0x19(%rdx,%r15,1)
   9ae7c:	0f 85 52 3f 00 00    	jne    9edd4 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x57d4>
   9ae82:	49 83 fd 04          	cmp    $0x4,%r13
   9ae86:	0f 85 b9 21 00 00    	jne    9d045 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x3a45>
   9ae8c:	48 8d bc 24 10 06 00 	lea    0x610(%rsp),%rdi
   9ae93:	00 
   9ae94:	48 8d b4 24 88 00 00 	lea    0x88(%rsp),%rsi
   9ae9b:	00 
   9ae9c:	e8 2f e2 ff ff       	call   990d0 <_Wfl.fl_obj_index$6801bb8d311b3e64>
   9aea1:	48 3b 45 08          	cmp    0x8(%rbp),%rax
   9aea5:	0f 83 42 3e 00 00    	jae    9eced <_Wfl.fl_vm_exec$a5081fe92876eb37+0x56ed>
   9aeab:	48 8b 4d 00          	mov    0x0(%rbp),%rcx
   9aeaf:	48 69 c0 60 01 00 00 	imul   $0x160,%rax,%rax
   9aeb6:	48 8b 4c 01 11       	mov    0x11(%rcx,%rax,1),%rcx
   9aebb:	e9 c2 2a 00 00       	jmp    9d982 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4382>
   9aec0:	4c 89 f7             	mov    %r14,%rdi
   9aec3:	48 8d b4 24 e8 00 00 	lea    0xe8(%rsp),%rsi
   9aeca:	00 
   9aecb:	e8 40 37 01 00       	call   ae610 <_Wfl.fl_pop$add7471f880d5ef3>
   9aed0:	4d 85 e4             	test   %r12,%r12
   9aed3:	0f 88 fb 00 00 00    	js     9afd4 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x19d4>
   9aed9:	49 8b 86 a4 00 00 00 	mov    0xa4(%r14),%rax
   9aee0:	4c 3b 60 08          	cmp    0x8(%rax),%r12
   9aee4:	0f 83 7a 3b 00 00    	jae    9ea64 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5464>
   9aeea:	48 8b 00             	mov    (%rax),%rax
   9aeed:	4b 8d 0c 64          	lea    (%r12,%r12,2),%rcx
   9aef1:	48 8b 94 24 e8 00 00 	mov    0xe8(%rsp),%rdx
   9aef8:	00 
   9aef9:	48 89 14 c8          	mov    %rdx,(%rax,%rcx,8)
   9aefd:	48 8b 94 24 f0 00 00 	mov    0xf0(%rsp),%rdx
   9af04:	00 
   9af05:	48 89 54 c8 08       	mov    %rdx,0x8(%rax,%rcx,8)
   9af0a:	f3 0f 7e 84 24 f8 00 	movq   0xf8(%rsp),%xmm0
   9af11:	00 00 
   9af13:	66 0f d6 44 c8 10    	movq   %xmm0,0x10(%rax,%rcx,8)
   9af19:	e9 02 01 00 00       	jmp    9b020 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1a20>
   9af1e:	49 8d 44 24 ef       	lea    -0x11(%r12),%rax
   9af23:	48 83 f8 04          	cmp    $0x4,%rax
   9af27:	0f 87 5e 02 00 00    	ja     9b18b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1b8b>
   9af2d:	4c 89 f7             	mov    %r14,%rdi
   9af30:	48 8d 9c 24 a8 21 00 	lea    0x21a8(%rsp),%rbx
   9af37:	00 
   9af38:	48 89 de             	mov    %rbx,%rsi
   9af3b:	e8 d0 36 01 00       	call   ae610 <_Wfl.fl_pop$add7471f880d5ef3>
   9af40:	4c 89 f7             	mov    %r14,%rdi
   9af43:	4c 8d bc 24 90 21 00 	lea    0x2190(%rsp),%r15
   9af4a:	00 
   9af4b:	4c 89 fe             	mov    %r15,%rsi
   9af4e:	e8 bd 36 01 00       	call   ae610 <_Wfl.fl_pop$add7471f880d5ef3>
   9af53:	4c 89 f7             	mov    %r14,%rdi
   9af56:	4c 89 fe             	mov    %r15,%rsi
   9af59:	48 89 da             	mov    %rbx,%rdx
   9af5c:	4c 89 e1             	mov    %r12,%rcx
   9af5f:	48 8d 9c 24 78 21 00 	lea    0x2178(%rsp),%rbx
   9af66:	00 
   9af67:	49 89 d8             	mov    %rbx,%r8
   9af6a:	e8 d1 8a 05 00       	call   f3a40 <_Wfl.fl_arith$c1c83b23f6345462>
   9af6f:	41 80 be 5c 01 00 00 	cmpb   $0x0,0x15c(%r14)
   9af76:	00 
   9af77:	0f 84 17 03 00 00    	je     9b294 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1c94>
   9af7d:	48 8b 6c 24 18       	mov    0x18(%rsp),%rbp
   9af82:	e9 99 00 00 00       	jmp    9b020 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1a20>
   9af87:	48 8d 91 09 01 00 00 	lea    0x109(%rcx),%rdx
   9af8e:	48 8d 81 01 01 00 00 	lea    0x101(%rcx),%rax
   9af95:	48 81 c1 f9 00 00 00 	add    $0xf9,%rcx
   9af9c:	48 8b 5c 24 08       	mov    0x8(%rsp),%rbx
   9afa1:	48 8b 12             	mov    (%rdx),%rdx
   9afa4:	48 8b 00             	mov    (%rax),%rax
   9afa7:	48 8b 09             	mov    (%rcx),%rcx
   9afaa:	48 89 8c 24 e8 02 00 	mov    %rcx,0x2e8(%rsp)
   9afb1:	00 
   9afb2:	48 89 84 24 f0 02 00 	mov    %rax,0x2f0(%rsp)
   9afb9:	00 
   9afba:	48 89 94 24 f8 02 00 	mov    %rdx,0x2f8(%rsp)
   9afc1:	00 
   9afc2:	4c 89 f7             	mov    %r14,%rdi
   9afc5:	48 8d b4 24 e8 02 00 	lea    0x2e8(%rsp),%rsi
   9afcc:	00 
   9afcd:	e8 0e 45 03 00       	call   cf4e0 <_Wfl.fl_push$396d177d8e34f937>
   9afd2:	eb 51                	jmp    9b025 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1a25>
   9afd4:	49 8b 06             	mov    (%r14),%rax
   9afd7:	4c 3b 78 08          	cmp    0x8(%rax),%r15
   9afdb:	0f 83 c5 3a 00 00    	jae    9eaa6 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x54a6>
   9afe1:	48 8b 00             	mov    (%rax),%rax
   9afe4:	48 8b 8c 24 e8 00 00 	mov    0xe8(%rsp),%rcx
   9afeb:	00 
   9afec:	48 89 8c 18 f9 00 00 	mov    %rcx,0xf9(%rax,%rbx,1)
   9aff3:	00 
   9aff4:	48 8b 8c 24 f0 00 00 	mov    0xf0(%rsp),%rcx
   9affb:	00 
   9affc:	48 89 8c 18 01 01 00 	mov    %rcx,0x101(%rax,%rbx,1)
   9b003:	00 
   9b004:	f3 0f 7e 84 24 f8 00 	movq   0xf8(%rsp),%xmm0
   9b00b:	00 00 
   9b00d:	66 0f d6 84 18 09 01 	movq   %xmm0,0x109(%rax,%rbx,1)
   9b014:	00 00 
   9b016:	66 2e 0f 1f 84 00 00 	cs nopw 0x0(%rax,%rax,1)
   9b01d:	00 00 00 
   9b020:	48 8b 5c 24 08       	mov    0x8(%rsp),%rbx
   9b025:	41 80 be 5c 01 00 00 	cmpb   $0x0,0x15c(%r14)
   9b02c:	00 
   9b02d:	0f 84 ed e6 ff ff    	je     99720 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x120>
   9b033:	49 8b 96 b4 00 00 00 	mov    0xb4(%r14),%rdx
   9b03a:	49 8b 86 bc 00 00 00 	mov    0xbc(%r14),%rax
   9b041:	48 8b 48 08          	mov    0x8(%rax),%rcx
   9b045:	48 3b 6a 08          	cmp    0x8(%rdx),%rbp
   9b049:	7d 25                	jge    9b070 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1a70>
   9b04b:	0f 83 8d 37 00 00    	jae    9e7de <_Wfl.fl_vm_exec$a5081fe92876eb37+0x51de>
   9b051:	48 8b 12             	mov    (%rdx),%rdx
   9b054:	48 8b b4 24 10 01 00 	mov    0x110(%rsp),%rsi
   9b05b:	00 
   9b05c:	48 8b 54 32 38       	mov    0x38(%rdx,%rsi,1),%rdx
   9b061:	48 85 c9             	test   %rcx,%rcx
   9b064:	75 15                	jne    9b07b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1a7b>
   9b066:	e9 f5 34 00 00       	jmp    9e560 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4f60>
   9b06b:	0f 1f 44 00 00       	nopl   0x0(%rax,%rax,1)
   9b070:	31 d2                	xor    %edx,%edx
   9b072:	48 85 c9             	test   %rcx,%rcx
   9b075:	0f 84 e5 34 00 00    	je     9e560 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4f60>
   9b07b:	48 89 ce             	mov    %rcx,%rsi
   9b07e:	48 ff ce             	dec    %rsi
   9b081:	0f 80 5a 35 00 00    	jo     9e5e1 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4fe1>
   9b087:	48 39 ce             	cmp    %rcx,%rsi
   9b08a:	0f 83 e0 36 00 00    	jae    9e770 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5170>
   9b090:	48 8b 38             	mov    (%rax),%rdi
   9b093:	49 89 f0             	mov    %rsi,%r8
   9b096:	49 c1 e0 05          	shl    $0x5,%r8
   9b09a:	4a 39 6c 07 10       	cmp    %rbp,0x10(%rdi,%r8,1)
   9b09f:	0f 8e cc 34 00 00    	jle    9e571 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4f71>
   9b0a5:	49 89 dd             	mov    %rbx,%r13
   9b0a8:	4a 8b 54 07 18       	mov    0x18(%rdi,%r8,1),%rdx
   9b0ad:	49 89 96 b5 01 00 00 	mov    %rdx,0x1b5(%r14)
   9b0b4:	48 85 c9             	test   %rcx,%rcx
   9b0b7:	7e 17                	jle    9b0d0 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1ad0>
   9b0b9:	48 89 70 08          	mov    %rsi,0x8(%rax)
   9b0bd:	4e 8b 24 07          	mov    (%rdi,%r8,1),%r12
   9b0c1:	4a 8b 5c 07 08       	mov    0x8(%rdi,%r8,1),%rbx
   9b0c6:	4e 8b 7c 07 10       	mov    0x10(%rdi,%r8,1),%r15
   9b0cb:	eb 0b                	jmp    9b0d8 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1ad8>
   9b0cd:	0f 1f 00             	nopl   (%rax)
   9b0d0:	45 31 ff             	xor    %r15d,%r15d
   9b0d3:	31 db                	xor    %ebx,%ebx
   9b0d5:	45 31 e4             	xor    %r12d,%r12d
   9b0d8:	4c 89 f7             	mov    %r14,%rdi
   9b0db:	48 89 de             	mov    %rbx,%rsi
   9b0de:	e8 9d 44 03 00       	call   cf580 <_Wfl.fl_close_upvals$345768e8f0248c70>
   9b0e3:	49 8b 86 b4 00 00 00 	mov    0xb4(%r14),%rax
   9b0ea:	48 8b 48 08          	mov    0x8(%rax),%rcx
   9b0ee:	66 90                	xchg   %ax,%ax
   9b0f0:	4c 39 f9             	cmp    %r15,%rcx
   9b0f3:	7e 1b                	jle    9b110 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1b10>
   9b0f5:	48 85 c9             	test   %rcx,%rcx
   9b0f8:	7e f6                	jle    9b0f0 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1af0>
   9b0fa:	48 ff c9             	dec    %rcx
   9b0fd:	48 89 48 08          	mov    %rcx,0x8(%rax)
   9b101:	eb ed                	jmp    9b0f0 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1af0>
   9b103:	66 66 66 66 2e 0f 1f 	data16 data16 data16 cs nopw 0x0(%rax,%rax,1)
   9b10a:	84 00 00 00 00 00 
   9b110:	49 89 9e ac 00 00 00 	mov    %rbx,0xac(%r14)
   9b117:	f3 41 0f 7e 86 6d 01 	movq   0x16d(%r14),%xmm0
   9b11e:	00 00 
   9b120:	41 0f 10 8e 5d 01 00 	movups 0x15d(%r14),%xmm1
   9b127:	00 
   9b128:	0f 29 8c 24 20 03 00 	movaps %xmm1,0x320(%rsp)
   9b12f:	00 
   9b130:	66 0f d6 84 24 30 03 	movq   %xmm0,0x330(%rsp)
   9b137:	00 00 
   9b139:	66 0f ef c0          	pxor   %xmm0,%xmm0
   9b13d:	49 8d 86 5c 01 00 00 	lea    0x15c(%r14),%rax
   9b144:	f3 0f 7f 40 09       	movdqu %xmm0,0x9(%rax)
   9b149:	f3 0f 7f 00          	movdqu %xmm0,(%rax)
   9b14d:	4c 89 f7             	mov    %r14,%rdi
   9b150:	48 8d b4 24 20 03 00 	lea    0x320(%rsp),%rsi
   9b157:	00 
   9b158:	e8 83 43 03 00       	call   cf4e0 <_Wfl.fl_push$396d177d8e34f937>
   9b15d:	49 ff cf             	dec    %r15
   9b160:	0f 80 8e 34 00 00    	jo     9e5f4 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4ff4>
   9b166:	49 8b 86 b4 00 00 00 	mov    0xb4(%r14),%rax
   9b16d:	4c 3b 78 08          	cmp    0x8(%rax),%r15
   9b171:	0f 83 0f 36 00 00    	jae    9e786 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5186>
   9b177:	48 8b 00             	mov    (%rax),%rax
   9b17a:	49 c1 e7 06          	shl    $0x6,%r15
   9b17e:	4e 89 64 38 18       	mov    %r12,0x18(%rax,%r15,1)
   9b183:	4c 89 eb             	mov    %r13,%rbx
   9b186:	e9 95 e5 ff ff       	jmp    99720 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x120>
   9b18b:	49 83 fc 17          	cmp    $0x17,%r12
   9b18f:	74 60                	je     9b1f1 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1bf1>
   9b191:	49 83 fc 16          	cmp    $0x16,%r12
   9b195:	0f 85 9e 00 00 00    	jne    9b239 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1c39>
   9b19b:	49 ff ca             	dec    %r10
   9b19e:	0f 80 24 36 00 00    	jo     9e7c8 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x51c8>
   9b1a4:	4c 3b 56 08          	cmp    0x8(%rsi),%r10
   9b1a8:	48 8b 6c 24 18       	mov    0x18(%rsp),%rbp
   9b1ad:	0f 83 14 39 00 00    	jae    9eac7 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x54c7>
   9b1b3:	48 8b 06             	mov    (%rsi),%rax
   9b1b6:	49 c1 e2 03          	shl    $0x3,%r10
   9b1ba:	4b 8d 0c 52          	lea    (%r10,%r10,2),%rcx
   9b1be:	48 8b 1c 08          	mov    (%rax,%rcx,1),%rbx
   9b1c2:	48 83 fb 03          	cmp    $0x3,%rbx
   9b1c6:	0f 84 62 01 00 00    	je     9b32e <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1d2e>
   9b1cc:	48 83 fb 02          	cmp    $0x2,%rbx
   9b1d0:	0f 85 73 01 00 00    	jne    9b349 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1d49>
   9b1d6:	48 c7 04 08 02 00 00 	movq   $0x2,(%rax,%rcx,1)
   9b1dd:	00 
   9b1de:	48 f7 5c 08 08       	negq   0x8(%rax,%rcx,1)
   9b1e3:	48 c7 44 08 10 00 00 	movq   $0x0,0x10(%rax,%rcx,1)
   9b1ea:	00 00 
   9b1ec:	e9 2f fe ff ff       	jmp    9b020 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1a20>
   9b1f1:	49 ff ca             	dec    %r10
   9b1f4:	0f 80 b8 35 00 00    	jo     9e7b2 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x51b2>
   9b1fa:	4c 3b 56 08          	cmp    0x8(%rsi),%r10
   9b1fe:	48 8b 6c 24 18       	mov    0x18(%rsp),%rbp
   9b203:	48 8b 5c 24 08       	mov    0x8(%rsp),%rbx
   9b208:	0f 83 cf 38 00 00    	jae    9eadd <_Wfl.fl_vm_exec$a5081fe92876eb37+0x54dd>
   9b20e:	48 8b 06             	mov    (%rsi),%rax
   9b211:	49 c1 e2 03          	shl    $0x3,%r10
   9b215:	4b 8d 0c 52          	lea    (%r10,%r10,2),%rcx
   9b219:	48 8b 34 08          	mov    (%rax,%rcx,1),%rsi
   9b21d:	48 85 f6             	test   %rsi,%rsi
   9b220:	0f 84 e8 00 00 00    	je     9b30e <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1d0e>
   9b226:	48 83 f6 01          	xor    $0x1,%rsi
   9b22a:	31 d2                	xor    %edx,%edx
   9b22c:	48 0b 74 08 08       	or     0x8(%rax,%rcx,1),%rsi
   9b231:	0f 94 c2             	sete   %dl
   9b234:	e9 da 00 00 00       	jmp    9b313 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1d13>
   9b239:	49 8d 44 24 e8       	lea    -0x18(%r12),%rax
   9b23e:	48 83 f8 05          	cmp    $0x5,%rax
   9b242:	77 5b                	ja     9b29f <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1c9f>
   9b244:	4c 89 f7             	mov    %r14,%rdi
   9b247:	48 8d 9c 24 48 21 00 	lea    0x2148(%rsp),%rbx
   9b24e:	00 
   9b24f:	48 89 de             	mov    %rbx,%rsi
   9b252:	e8 b9 33 01 00       	call   ae610 <_Wfl.fl_pop$add7471f880d5ef3>
   9b257:	4c 89 f7             	mov    %r14,%rdi
   9b25a:	4c 8d bc 24 30 21 00 	lea    0x2130(%rsp),%r15
   9b261:	00 
   9b262:	4c 89 fe             	mov    %r15,%rsi
   9b265:	e8 a6 33 01 00       	call   ae610 <_Wfl.fl_pop$add7471f880d5ef3>
   9b26a:	4c 89 f7             	mov    %r14,%rdi
   9b26d:	4c 89 fe             	mov    %r15,%rsi
   9b270:	48 89 da             	mov    %rbx,%rdx
   9b273:	4c 89 e1             	mov    %r12,%rcx
   9b276:	48 8d 9c 24 18 21 00 	lea    0x2118(%rsp),%rbx
   9b27d:	00 
   9b27e:	49 89 d8             	mov    %rbx,%r8
   9b281:	e8 8a 9c 05 00       	call   f4f10 <_Wfl.fl_cmp$e8c241740a9fb998>
   9b286:	41 80 be 5c 01 00 00 	cmpb   $0x0,0x15c(%r14)
   9b28d:	00 
   9b28e:	0f 85 e9 fc ff ff    	jne    9af7d <_Wfl.fl_vm_exec$a5081fe92876eb37+0x197d>
   9b294:	4c 89 f7             	mov    %r14,%rdi
   9b297:	48 89 de             	mov    %rbx,%rsi
   9b29a:	e9 2d f2 ff ff       	jmp    9a4cc <_Wfl.fl_vm_exec$a5081fe92876eb37+0xecc>
   9b29f:	49 8d 44 24 e2       	lea    -0x1e(%r12),%rax
   9b2a4:	48 83 f8 05          	cmp    $0x5,%rax
   9b2a8:	0f 87 0f 01 00 00    	ja     9b3bd <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1dbd>
   9b2ae:	4c 89 f7             	mov    %r14,%rdi
   9b2b1:	4c 89 ee             	mov    %r13,%rsi
   9b2b4:	48 8b 54 24 08       	mov    0x8(%rsp),%rdx
   9b2b9:	e8 02 61 02 00       	call   c13c0 <_Wfl.fl_read16$7e28ba906e98be2a>
   9b2be:	48 89 c5             	mov    %rax,%rbp
   9b2c1:	49 83 fc 20          	cmp    $0x20,%r12
   9b2c5:	0f 95 c0             	setne  %al
   9b2c8:	49 83 fc 21          	cmp    $0x21,%r12
   9b2cc:	0f 94 c3             	sete   %bl
   9b2cf:	30 d8                	xor    %bl,%al
   9b2d1:	b1 01                	mov    $0x1,%cl
   9b2d3:	0f 85 5d 02 00 00    	jne    9b536 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1f36>
   9b2d9:	4c 89 f7             	mov    %r14,%rdi
   9b2dc:	48 8d b4 24 f0 04 00 	lea    0x4f0(%rsp),%rsi
   9b2e3:	00 
   9b2e4:	e8 27 33 01 00       	call   ae610 <_Wfl.fl_pop$add7471f880d5ef3>
   9b2e9:	48 8b 84 24 f0 04 00 	mov    0x4f0(%rsp),%rax
   9b2f0:	00 
   9b2f1:	48 85 c0             	test   %rax,%rax
   9b2f4:	0f 84 35 02 00 00    	je     9b52f <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1f2f>
   9b2fa:	48 83 f0 01          	xor    $0x1,%rax
   9b2fe:	48 0b 84 24 f8 04 00 	or     0x4f8(%rsp),%rax
   9b305:	00 
   9b306:	0f 95 c0             	setne  %al
   9b309:	e9 23 02 00 00       	jmp    9b531 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1f31>
   9b30e:	ba 01 00 00 00       	mov    $0x1,%edx
   9b313:	48 c7 04 08 01 00 00 	movq   $0x1,(%rax,%rcx,1)
   9b31a:	00 
   9b31b:	48 89 54 08 08       	mov    %rdx,0x8(%rax,%rcx,1)
   9b320:	48 c7 44 08 10 00 00 	movq   $0x0,0x10(%rax,%rcx,1)
   9b327:	00 00 
   9b329:	e9 f7 fc ff ff       	jmp    9b025 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1a25>
   9b32e:	48 c7 04 08 03 00 00 	movq   $0x3,(%rax,%rcx,1)
   9b335:	00 
   9b336:	48 c7 44 08 08 00 00 	movq   $0x0,0x8(%rax,%rcx,1)
   9b33d:	00 00 
   9b33f:	80 74 08 17 80       	xorb   $0x80,0x17(%rax,%rcx,1)
   9b344:	e9 d7 fc ff ff       	jmp    9b020 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1a20>
   9b349:	e8 d2 d1 0c 00       	call   168520 <__wolf_rt_strbuf_new>
   9b34e:	49 89 c5             	mov    %rax,%r13
   9b351:	ba 0e 00 00 00       	mov    $0xe,%edx
   9b356:	48 89 c7             	mov    %rax,%rdi
   9b359:	48 8d 35 15 96 f7 ff 	lea    -0x869eb(%rip),%rsi        # 14975 <__abi_tag+0x14679>
   9b360:	31 c9                	xor    %ecx,%ecx
   9b362:	e8 69 d2 0c 00       	call   1685d0 <__wolf_rt_strbuf_str>
   9b367:	48 89 df             	mov    %rbx,%rdi
   9b36a:	e8 91 af 04 00       	call   e6300 <_Wfl.fl_type_name$425de879c2cc2a41>
   9b36f:	4c 89 ef             	mov    %r13,%rdi
   9b372:	48 89 c6             	mov    %rax,%rsi
   9b375:	31 c9                	xor    %ecx,%ecx
   9b377:	e8 54 d2 0c 00       	call   1685d0 <__wolf_rt_strbuf_str>
   9b37c:	4c 89 ef             	mov    %r13,%rdi
   9b37f:	48 8d b4 24 d0 01 00 	lea    0x1d0(%rsp),%rsi
   9b386:	00 
   9b387:	e8 44 ce 0c 00       	call   1681d0 <__wolf_rt_strbuf_finish>
   9b38c:	48 8b 8c 24 d0 01 00 	mov    0x1d0(%rsp),%rcx
   9b393:	00 
   9b394:	4c 8b 84 24 d8 01 00 	mov    0x1d8(%rsp),%r8
   9b39b:	00 
   9b39c:	ba 04 00 00 00       	mov    $0x4,%edx
   9b3a1:	4c 89 f7             	mov    %r14,%rdi
   9b3a4:	48 8d 35 35 3d f8 ff 	lea    -0x7c2cb(%rip),%rsi        # 1f0e0 <__abi_tag+0x1ede4>
   9b3ab:	4c 8d 8c 24 60 21 00 	lea    0x2160(%rsp),%r9
   9b3b2:	00 
   9b3b3:	e8 48 34 07 00       	call   10e800 <_Wfl.fl_raise_bytes.bytesview.4$e8b5fa06fc8adc2d>
   9b3b8:	e9 63 fc ff ff       	jmp    9b020 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1a20>
   9b3bd:	49 83 fc 24          	cmp    $0x24,%r12
   9b3c1:	0f 85 c0 00 00 00    	jne    9b487 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1e87>
   9b3c7:	4c 89 f7             	mov    %r14,%rdi
   9b3ca:	4c 89 ee             	mov    %r13,%rsi
   9b3cd:	48 8b 54 24 08       	mov    0x8(%rsp),%rdx
   9b3d2:	e8 39 5f 02 00       	call   c1310 <_Wfl.fl_read8$a2ddcd4dfd790b8d>
   9b3d7:	49 8b 9e ac 00 00 00 	mov    0xac(%r14),%rbx
   9b3de:	48 29 c3             	sub    %rax,%rbx
   9b3e1:	0f 80 c5 34 00 00    	jo     9e8ac <_Wfl.fl_vm_exec$a5081fe92876eb37+0x52ac>
   9b3e7:	48 ff cb             	dec    %rbx
   9b3ea:	0f 80 bc 34 00 00    	jo     9e8ac <_Wfl.fl_vm_exec$a5081fe92876eb37+0x52ac>
   9b3f0:	49 8b 8e a4 00 00 00 	mov    0xa4(%r14),%rcx
   9b3f7:	48 3b 59 08          	cmp    0x8(%rcx),%rbx
   9b3fb:	48 8b 6c 24 18       	mov    0x18(%rsp),%rbp
   9b400:	4c 8b 44 24 28       	mov    0x28(%rsp),%r8
   9b405:	0f 83 7f 38 00 00    	jae    9ec8a <_Wfl.fl_vm_exec$a5081fe92876eb37+0x568a>
   9b40b:	48 8b 09             	mov    (%rcx),%rcx
   9b40e:	48 8d 14 5b          	lea    (%rbx,%rbx,2),%rdx
   9b412:	f3 0f 7e 44 d1 10    	movq   0x10(%rcx,%rdx,8),%xmm0
   9b418:	0f 10 0c d1          	movups (%rcx,%rdx,8),%xmm1
   9b41c:	48 8b 0c d1          	mov    (%rcx,%rdx,8),%rcx
   9b420:	0f 29 8c 24 20 02 00 	movaps %xmm1,0x220(%rsp)
   9b427:	00 
   9b428:	66 0f d6 84 24 30 02 	movq   %xmm0,0x230(%rsp)
   9b42f:	00 00 
   9b431:	48 83 f9 09          	cmp    $0x9,%rcx
   9b435:	0f 85 d6 00 00 00    	jne    9b511 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1f11>
   9b43b:	48 8d 53 01          	lea    0x1(%rbx),%rdx
   9b43f:	4c 89 f7             	mov    %r14,%rdi
   9b442:	48 8d b4 24 20 02 00 	lea    0x220(%rsp),%rsi
   9b449:	00 
   9b44a:	48 89 c1             	mov    %rax,%rcx
   9b44d:	45 31 c0             	xor    %r8d,%r8d
   9b450:	4c 8d 8c 24 f8 05 00 	lea    0x5f8(%rsp),%r9
   9b457:	00 
   9b458:	e8 d3 dc ff ff       	call   99130 <_Wfl.fl_native_invoke$a020102912b2bd4e>
   9b45d:	41 80 be 5c 01 00 00 	cmpb   $0x0,0x15c(%r14)
   9b464:	00 
   9b465:	0f 85 b5 fb ff ff    	jne    9b020 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1a20>
   9b46b:	49 89 9e ac 00 00 00 	mov    %rbx,0xac(%r14)
   9b472:	4c 89 f7             	mov    %r14,%rdi
   9b475:	48 8d b4 24 f8 05 00 	lea    0x5f8(%rsp),%rsi
   9b47c:	00 
   9b47d:	e8 5e 40 03 00       	call   cf4e0 <_Wfl.fl_push$396d177d8e34f937>
   9b482:	e9 99 fb ff ff       	jmp    9b020 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1a20>
   9b487:	49 8d 44 24 db       	lea    -0x25(%r12),%rax
   9b48c:	48 83 f8 13          	cmp    $0x13,%rax
   9b490:	0f 87 0f 03 00 00    	ja     9b7a5 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x21a5>
   9b496:	48 8d 0d 73 8c f7 ff 	lea    -0x8738d(%rip),%rcx        # 14110 <__abi_tag+0x13e14>
   9b49d:	48 63 04 81          	movslq (%rcx,%rax,4),%rax
   9b4a1:	48 01 c8             	add    %rcx,%rax
   9b4a4:	ff e0                	jmp    *%rax
   9b4a6:	48 83 7b 08 00       	cmpq   $0x0,0x8(%rbx)
   9b4ab:	48 8b 5c 24 20       	mov    0x20(%rsp),%rbx
   9b4b0:	75 2f                	jne    9b4e1 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1ee1>
   9b4b2:	ba 0c 00 00 00       	mov    $0xc,%edx
   9b4b7:	4c 89 f7             	mov    %r14,%rdi
   9b4ba:	48 8d 35 9e 93 f7 ff 	lea    -0x86c62(%rip),%rsi        # 1485f <__abi_tag+0x14563>
   9b4c1:	48 8b 4c 24 08       	mov    0x8(%rsp),%rcx
   9b4c6:	4c 8b 44 24 28       	mov    0x28(%rsp),%r8
   9b4cb:	49 89 d9             	mov    %rbx,%r9
   9b4ce:	6a 20                	push   $0x20
   9b4d0:	48 8d 05 29 6b f8 ff 	lea    -0x794d7(%rip),%rax        # 22000 <__abi_tag+0x21d04>
   9b4d7:	50                   	push   %rax
   9b4d8:	e8 23 a9 05 00       	call   f5e00 <_Wfl.fl_vm_bug_at$494622488d4f7894>
   9b4dd:	48 83 c4 10          	add    $0x10,%rsp
   9b4e1:	48 83 fb 25          	cmp    $0x25,%rbx
   9b4e5:	48 8b 6c 24 18       	mov    0x18(%rsp),%rbp
   9b4ea:	0f 85 ed 00 00 00    	jne    9b5dd <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1fdd>
   9b4f0:	4c 89 f7             	mov    %r14,%rdi
   9b4f3:	48 8d 9c 24 e0 05 00 	lea    0x5e0(%rsp),%rbx
   9b4fa:	00 
   9b4fb:	48 89 de             	mov    %rbx,%rsi
   9b4fe:	e8 0d 31 01 00       	call   ae610 <_Wfl.fl_pop$add7471f880d5ef3>
   9b503:	f3 0f 6f 84 24 e8 05 	movdqu 0x5e8(%rsp),%xmm0
   9b50a:	00 00 
   9b50c:	e9 d7 00 00 00       	jmp    9b5e8 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1fe8>
   9b511:	4c 89 f7             	mov    %r14,%rdi
   9b514:	48 8d b4 24 20 02 00 	lea    0x220(%rsp),%rsi
   9b51b:	00 
   9b51c:	48 89 c2             	mov    %rax,%rdx
   9b51f:	48 89 d9             	mov    %rbx,%rcx
   9b522:	45 31 c9             	xor    %r9d,%r9d
   9b525:	e8 f6 42 03 00       	call   cf820 <_Wfl.fl_frame_push$10db4ef30ec77391>
   9b52a:	e9 f1 fa ff ff       	jmp    9b020 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1a20>
   9b52f:	31 c0                	xor    %eax,%eax
   9b531:	38 d8                	cmp    %bl,%al
   9b533:	0f 94 c1             	sete   %cl
   9b536:	49 83 fc 22          	cmp    $0x22,%r12
   9b53a:	0f 94 c0             	sete   %al
   9b53d:	44 89 e2             	mov    %r12d,%edx
   9b540:	83 e2 3e             	and    $0x3e,%edx
   9b543:	83 fa 22             	cmp    $0x22,%edx
   9b546:	48 8b 5c 24 08       	mov    0x8(%rsp),%rbx
   9b54b:	75 4a                	jne    9b597 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1f97>
   9b54d:	49 8b 8e ac 00 00 00 	mov    0xac(%r14),%rcx
   9b554:	48 ff c9             	dec    %rcx
   9b557:	0f 80 ff 33 00 00    	jo     9e95c <_Wfl.fl_vm_exec$a5081fe92876eb37+0x535c>
   9b55d:	49 8b 96 a4 00 00 00 	mov    0xa4(%r14),%rdx
   9b564:	48 3b 4a 08          	cmp    0x8(%rdx),%rcx
   9b568:	0f 83 03 38 00 00    	jae    9ed71 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5771>
   9b56e:	48 8b 12             	mov    (%rdx),%rdx
   9b571:	48 c1 e1 03          	shl    $0x3,%rcx
   9b575:	48 8d 0c 49          	lea    (%rcx,%rcx,2),%rcx
   9b579:	48 8b 34 0a          	mov    (%rdx,%rcx,1),%rsi
   9b57d:	48 85 f6             	test   %rsi,%rsi
   9b580:	74 0e                	je     9b590 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1f90>
   9b582:	48 83 f6 01          	xor    $0x1,%rsi
   9b586:	48 0b 74 0a 08       	or     0x8(%rdx,%rcx,1),%rsi
   9b58b:	0f 95 c1             	setne  %cl
   9b58e:	eb 02                	jmp    9b592 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1f92>
   9b590:	31 c9                	xor    %ecx,%ecx
   9b592:	38 c1                	cmp    %al,%cl
   9b594:	0f 94 c1             	sete   %cl
   9b597:	84 c9                	test   %cl,%cl
   9b599:	74 38                	je     9b5d3 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1fd3>
   9b59b:	49 8b 86 b4 00 00 00 	mov    0xb4(%r14),%rax
   9b5a2:	4c 3b 68 08          	cmp    0x8(%rax),%r13
   9b5a6:	0f 83 f7 35 00 00    	jae    9eba3 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x55a3>
   9b5ac:	49 83 fc 1f          	cmp    $0x1f,%r12
   9b5b0:	48 8b 4c 24 38       	mov    0x38(%rsp),%rcx
   9b5b5:	75 09                	jne    9b5c0 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1fc0>
   9b5b7:	48 f7 dd             	neg    %rbp
   9b5ba:	0f 80 20 34 00 00    	jo     9e9e0 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x53e0>
   9b5c0:	48 8b 00             	mov    (%rax),%rax
   9b5c3:	48 03 6c 08 18       	add    0x18(%rax,%rcx,1),%rbp
   9b5c8:	0f 80 6d 33 00 00    	jo     9e93b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x533b>
   9b5ce:	48 89 6c 08 18       	mov    %rbp,0x18(%rax,%rcx,1)
   9b5d3:	48 8b 6c 24 18       	mov    0x18(%rsp),%rbp
   9b5d8:	e9 48 fa ff ff       	jmp    9b025 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1a25>
   9b5dd:	f3 0f 6f 44 24 58    	movdqu 0x58(%rsp),%xmm0
   9b5e3:	48 8d 5c 24 50       	lea    0x50(%rsp),%rbx
   9b5e8:	48 8b 03             	mov    (%rbx),%rax
   9b5eb:	48 89 84 24 68 01 00 	mov    %rax,0x168(%rsp)
   9b5f2:	00 
   9b5f3:	f3 0f 7f 84 24 70 01 	movdqu %xmm0,0x170(%rsp)
   9b5fa:	00 00 
   9b5fc:	49 8b 86 b4 00 00 00 	mov    0xb4(%r14),%rax
   9b603:	4c 3b 68 08          	cmp    0x8(%rax),%r13
   9b607:	0f 83 5e 38 00 00    	jae    9ee6b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x586b>
   9b60d:	48 8b 00             	mov    (%rax),%rax
   9b610:	48 8b 4c 24 38       	mov    0x38(%rsp),%rcx
   9b615:	48 8b 44 08 38       	mov    0x38(%rax,%rcx,1),%rax
   9b61a:	49 89 86 b5 01 00 00 	mov    %rax,0x1b5(%r14)
   9b621:	4c 89 f7             	mov    %r14,%rdi
   9b624:	48 8b 5c 24 40       	mov    0x40(%rsp),%rbx
   9b629:	48 89 de             	mov    %rbx,%rsi
   9b62c:	e8 4f 3f 03 00       	call   cf580 <_Wfl.fl_close_upvals$345768e8f0248c70>
   9b631:	4c 89 f7             	mov    %r14,%rdi
   9b634:	48 8d b4 24 38 22 00 	lea    0x2238(%rsp),%rsi
   9b63b:	00 
   9b63c:	e8 cf b3 04 00       	call   e6a10 <_Wfl.fl_return_frame$f89d02dfe330bb27>
   9b641:	49 8b 86 b4 00 00 00 	mov    0xb4(%r14),%rax
   9b648:	48 39 68 08          	cmp    %rbp,0x8(%rax)
   9b64c:	0f 84 bf 30 00 00    	je     9e711 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5111>
   9b652:	49 89 9e ac 00 00 00 	mov    %rbx,0xac(%r14)
   9b659:	4c 89 f7             	mov    %r14,%rdi
   9b65c:	48 8d b4 24 68 01 00 	lea    0x168(%rsp),%rsi
   9b663:	00 
   9b664:	e8 77 3e 03 00       	call   cf4e0 <_Wfl.fl_push$396d177d8e34f937>
   9b669:	e9 b2 f9 ff ff       	jmp    9b020 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1a20>
   9b66e:	4c 89 f7             	mov    %r14,%rdi
   9b671:	4c 89 ee             	mov    %r13,%rsi
   9b674:	48 8b 54 24 08       	mov    0x8(%rsp),%rdx
   9b679:	e8 42 5d 02 00       	call   c13c0 <_Wfl.fl_read16$7e28ba906e98be2a>
   9b67e:	48 89 c3             	mov    %rax,%rbx
   9b681:	be 06 00 00 00       	mov    $0x6,%esi
   9b686:	ba 38 00 00 00       	mov    $0x38,%edx
   9b68b:	4c 89 f7             	mov    %r14,%rdi
   9b68e:	48 8d 8c 24 c0 04 00 	lea    0x4c0(%rsp),%rcx
   9b695:	00 
   9b696:	e8 05 50 07 00       	call   1106a0 <_Wfl.fl_gc_alloc$34adf57915ed2b38>
   9b69b:	48 89 5c 24 28       	mov    %rbx,0x28(%rsp)
   9b6a0:	48 85 db             	test   %rbx,%rbx
   9b6a3:	0f 8e f2 05 00 00    	jle    9bc9b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x269b>
   9b6a9:	48 b8 00 00 00 00 00 	movabs $0x4000000000000000,%rax
   9b6b0:	00 00 40 
   9b6b3:	48 ff c8             	dec    %rax
   9b6b6:	48 39 44 24 28       	cmp    %rax,0x28(%rsp)
   9b6bb:	0f 87 7d 31 00 00    	ja     9e83e <_Wfl.fl_vm_exec$a5081fe92876eb37+0x523e>
   9b6c1:	48 8b 5c 24 28       	mov    0x28(%rsp),%rbx
   9b6c6:	4c 8d 3c 1b          	lea    (%rbx,%rbx,1),%r15
   9b6ca:	49 bc 00 00 00 00 00 	movabs $0x4000000000000000,%r12
   9b6d1:	00 00 40 
   9b6d4:	31 ed                	xor    %ebp,%ebp
   9b6d6:	49 8b 86 ac 00 00 00 	mov    0xac(%r14),%rax
   9b6dd:	4c 29 f8             	sub    %r15,%rax
   9b6e0:	0f 90 c1             	seto   %cl
   9b6e3:	4d 85 e4             	test   %r12,%r12
   9b6e6:	0f 88 52 31 00 00    	js     9e83e <_Wfl.fl_vm_exec$a5081fe92876eb37+0x523e>
   9b6ec:	84 c9                	test   %cl,%cl
   9b6ee:	0f 85 4a 31 00 00    	jne    9e83e <_Wfl.fl_vm_exec$a5081fe92876eb37+0x523e>
   9b6f4:	48 01 e8             	add    %rbp,%rax
   9b6f7:	0f 80 41 31 00 00    	jo     9e83e <_Wfl.fl_vm_exec$a5081fe92876eb37+0x523e>
   9b6fd:	49 8b 8e a4 00 00 00 	mov    0xa4(%r14),%rcx
   9b704:	48 8b 51 08          	mov    0x8(%rcx),%rdx
   9b708:	48 39 d0             	cmp    %rdx,%rax
   9b70b:	0f 83 a8 34 00 00    	jae    9ebb9 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x55b9>
   9b711:	48 8b 09             	mov    (%rcx),%rcx
   9b714:	48 8d 34 40          	lea    (%rax,%rax,2),%rsi
   9b718:	f3 0f 7e 44 f1 10    	movq   0x10(%rcx,%rsi,8),%xmm0
   9b71e:	0f 10 0c f1          	movups (%rcx,%rsi,8),%xmm1
   9b722:	4c 8b 2c f1          	mov    (%rcx,%rsi,8),%r13
   9b726:	0f 29 8c 24 c0 03 00 	movaps %xmm1,0x3c0(%rsp)
   9b72d:	00 
   9b72e:	66 0f d6 84 24 d0 03 	movq   %xmm0,0x3d0(%rsp)
   9b735:	00 00 
   9b737:	48 ff c0             	inc    %rax
   9b73a:	0f 80 da 31 00 00    	jo     9e91a <_Wfl.fl_vm_exec$a5081fe92876eb37+0x531a>
   9b740:	48 39 d0             	cmp    %rdx,%rax
   9b743:	0f 83 9c 34 00 00    	jae    9ebe5 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x55e5>
   9b749:	48 8d 04 40          	lea    (%rax,%rax,2),%rax
   9b74d:	f3 0f 7e 44 c1 10    	movq   0x10(%rcx,%rax,8),%xmm0
   9b753:	0f 10 0c c1          	movups (%rcx,%rax,8),%xmm1
   9b757:	0f 29 8c 24 a0 03 00 	movaps %xmm1,0x3a0(%rsp)
   9b75e:	00 
   9b75f:	66 0f d6 84 24 b0 03 	movq   %xmm0,0x3b0(%rsp)
   9b766:	00 00 
   9b768:	4c 89 f7             	mov    %r14,%rdi
   9b76b:	48 8d b4 24 c0 04 00 	lea    0x4c0(%rsp),%rsi
   9b772:	00 
   9b773:	48 8d 94 24 c0 03 00 	lea    0x3c0(%rsp),%rdx
   9b77a:	00 
   9b77b:	48 8d 8c 24 a0 03 00 	lea    0x3a0(%rsp),%rcx
   9b782:	00 
   9b783:	e8 68 7e ff ff       	call   935f0 <_Wfl.fl_map_set$540fcc92758450a0>
   9b788:	84 c0                	test   %al,%al
   9b78a:	0f 84 9c 04 00 00    	je     9bc2c <_Wfl.fl_vm_exec$a5081fe92876eb37+0x262c>
   9b790:	48 83 c5 02          	add    $0x2,%rbp
   9b794:	49 ff c4             	inc    %r12
   9b797:	48 ff cb             	dec    %rbx
   9b79a:	0f 85 36 ff ff ff    	jne    9b6d6 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x20d6>
   9b7a0:	e9 f6 04 00 00       	jmp    9bc9b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x269b>
   9b7a5:	48 8b 44 24 20       	mov    0x20(%rsp),%rax
   9b7aa:	48 83 e0 fe          	and    $0xfffffffffffffffe,%rax
   9b7ae:	48 83 f8 2a          	cmp    $0x2a,%rax
   9b7b2:	0f 85 ef 03 00 00    	jne    9bba7 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x25a7>
   9b7b8:	48 83 7c 24 20 2b    	cmpq   $0x2b,0x20(%rsp)
   9b7be:	0f 85 36 05 00 00    	jne    9bcfa <_Wfl.fl_vm_exec$a5081fe92876eb37+0x26fa>
   9b7c4:	48 8b 7c 24 10       	mov    0x10(%rsp),%rdi
   9b7c9:	48 8d 9c 24 c8 05 00 	lea    0x5c8(%rsp),%rbx
   9b7d0:	00 
   9b7d1:	48 89 de             	mov    %rbx,%rsi
   9b7d4:	e8 37 2e 01 00       	call   ae610 <_Wfl.fl_pop$add7471f880d5ef3>
   9b7d9:	f3 0f 6f 84 24 d0 05 	movdqu 0x5d0(%rsp),%xmm0
   9b7e0:	00 00 
   9b7e2:	e9 1e 05 00 00       	jmp    9bd05 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x2705>
   9b7e7:	4c 89 f7             	mov    %r14,%rdi
   9b7ea:	4c 89 6c 24 20       	mov    %r13,0x20(%rsp)
   9b7ef:	4c 89 ee             	mov    %r13,%rsi
   9b7f2:	48 8b 5c 24 08       	mov    0x8(%rsp),%rbx
   9b7f7:	48 89 da             	mov    %rbx,%rdx
   9b7fa:	e8 c1 5b 02 00       	call   c13c0 <_Wfl.fl_read16$7e28ba906e98be2a>
   9b7ff:	49 8b 36             	mov    (%r14),%rsi
   9b802:	48 8b 4e 08          	mov    0x8(%rsi),%rcx
   9b806:	48 39 cb             	cmp    %rcx,%rbx
   9b809:	0f 83 f4 34 00 00    	jae    9ed03 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5703>
   9b80f:	48 8b 16             	mov    (%rsi),%rdx
   9b812:	48 69 fb 60 01 00 00 	imul   $0x160,%rbx,%rdi
   9b819:	48 8b 7c 3a 29       	mov    0x29(%rdx,%rdi,1),%rdi
   9b81e:	48 3b 47 08          	cmp    0x8(%rdi),%rax
   9b822:	0f 83 db 34 00 00    	jae    9ed03 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5703>
   9b828:	4c 8b 07             	mov    (%rdi),%r8
   9b82b:	4c 8d 0c 40          	lea    (%rax,%rax,2),%r9
   9b82f:	4b 8b 3c c8          	mov    (%r8,%r9,8),%rdi
   9b833:	4b 8b 44 c8 08       	mov    0x8(%r8,%r9,8),%rax
   9b838:	f3 43 0f 7e 44 c8 10 	movq   0x10(%r8,%r9,8),%xmm0
   9b83f:	48 89 bc 24 70 02 00 	mov    %rdi,0x270(%rsp)
   9b846:	00 
   9b847:	48 89 84 24 78 02 00 	mov    %rax,0x278(%rsp)
   9b84e:	00 
   9b84f:	66 0f d6 84 24 80 02 	movq   %xmm0,0x280(%rsp)
   9b856:	00 00 
   9b858:	4c 8d 47 f4          	lea    -0xc(%rdi),%r8
   9b85c:	49 83 f8 f8          	cmp    $0xfffffffffffffff8,%r8
   9b860:	0f 82 2c 35 00 00    	jb     9ed92 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5792>
   9b866:	48 85 c0             	test   %rax,%rax
   9b869:	0f 88 23 35 00 00    	js     9ed92 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5792>
   9b86f:	41 89 c0             	mov    %eax,%r8d
   9b872:	4c 3b 46 08          	cmp    0x8(%rsi),%r8
   9b876:	0f 8d 16 35 00 00    	jge    9ed92 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5792>
   9b87c:	4c 8b 0e             	mov    (%rsi),%r9
   9b87f:	49 69 f0 60 01 00 00 	imul   $0x160,%r8,%rsi
   9b886:	49 39 3c 31          	cmp    %rdi,(%r9,%rsi,1)
   9b88a:	0f 85 02 35 00 00    	jne    9ed92 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5792>
   9b890:	48 c1 e8 20          	shr    $0x20,%rax
   9b894:	49 39 44 31 19       	cmp    %rax,0x19(%r9,%rsi,1)
   9b899:	0f 85 f3 34 00 00    	jne    9ed92 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5792>
   9b89f:	49 39 c8             	cmp    %rcx,%r8
   9b8a2:	0f 83 00 35 00 00    	jae    9eda8 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x57a8>
   9b8a8:	4c 8b bc 32 89 00 00 	mov    0x89(%rdx,%rsi,1),%r15
   9b8af:	00 
   9b8b0:	4c 8b 74 24 10       	mov    0x10(%rsp),%r14
   9b8b5:	4c 89 f7             	mov    %r14,%rdi
   9b8b8:	48 8d b4 24 70 02 00 	lea    0x270(%rsp),%rsi
   9b8bf:	00 
   9b8c0:	48 8d 94 24 58 02 00 	lea    0x258(%rsp),%rdx
   9b8c7:	00 
   9b8c8:	e8 c3 11 08 00       	call   11ca90 <_Wfl.fl_closure_new$04ec049e49b3b2ad>
   9b8cd:	48 8b 84 24 58 02 00 	mov    0x258(%rsp),%rax
   9b8d4:	00 
   9b8d5:	48 8d 48 fc          	lea    -0x4(%rax),%rcx
   9b8d9:	48 83 f9 07          	cmp    $0x7,%rcx
   9b8dd:	0f 87 db 34 00 00    	ja     9edbe <_Wfl.fl_vm_exec$a5081fe92876eb37+0x57be>
   9b8e3:	48 8b 94 24 60 02 00 	mov    0x260(%rsp),%rdx
   9b8ea:	00 
   9b8eb:	48 85 d2             	test   %rdx,%rdx
   9b8ee:	0f 88 ca 34 00 00    	js     9edbe <_Wfl.fl_vm_exec$a5081fe92876eb37+0x57be>
   9b8f4:	49 8b 0e             	mov    (%r14),%rcx
   9b8f7:	41 89 d4             	mov    %edx,%r12d
   9b8fa:	48 8b 71 08          	mov    0x8(%rcx),%rsi
   9b8fe:	49 39 f4             	cmp    %rsi,%r12
   9b901:	0f 8d b7 34 00 00    	jge    9edbe <_Wfl.fl_vm_exec$a5081fe92876eb37+0x57be>
   9b907:	48 8b 09             	mov    (%rcx),%rcx
   9b90a:	49 69 ec 60 01 00 00 	imul   $0x160,%r12,%rbp
   9b911:	48 39 04 29          	cmp    %rax,(%rcx,%rbp,1)
   9b915:	0f 85 a3 34 00 00    	jne    9edbe <_Wfl.fl_vm_exec$a5081fe92876eb37+0x57be>
   9b91b:	48 c1 ea 20          	shr    $0x20,%rdx
   9b91f:	48 39 54 29 19       	cmp    %rdx,0x19(%rcx,%rbp,1)
   9b924:	0f 85 94 34 00 00    	jne    9edbe <_Wfl.fl_vm_exec$a5081fe92876eb37+0x57be>
   9b92a:	48 8b 54 24 48       	mov    0x48(%rsp),%rdx
   9b92f:	48 39 f2             	cmp    %rsi,%rdx
   9b932:	0f 83 db 34 00 00    	jae    9ee13 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5813>
   9b938:	49 8b 46 40          	mov    0x40(%r14),%rax
   9b93c:	48 69 d2 60 01 00 00 	imul   $0x160,%rdx,%rdx
   9b943:	f3 0f 7e 84 11 f1 00 	movq   0xf1(%rcx,%rdx,1),%xmm0
   9b94a:	00 00 
   9b94c:	48 89 54 24 28       	mov    %rdx,0x28(%rsp)
   9b951:	0f 10 8c 11 e1 00 00 	movups 0xe1(%rcx,%rdx,1),%xmm1
   9b958:	00 
   9b959:	0f 11 8c 29 e1 00 00 	movups %xmm1,0xe1(%rcx,%rbp,1)
   9b960:	00 
   9b961:	66 0f d6 84 29 f1 00 	movq   %xmm0,0xf1(%rcx,%rbp,1)
   9b968:	00 00 
   9b96a:	4c 89 bc 29 89 00 00 	mov    %r15,0x89(%rcx,%rbp,1)
   9b971:	00 
   9b972:	48 b9 00 00 00 00 00 	movabs $0xf000000000000000,%rcx
   9b979:	00 00 f0 
   9b97c:	4c 01 f9             	add    %r15,%rcx
   9b97f:	48 c1 e9 3d          	shr    $0x3d,%rcx
   9b983:	83 f9 07             	cmp    $0x7,%ecx
   9b986:	0f 82 75 30 00 00    	jb     9ea01 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5401>
   9b98c:	4a 8d 0c fd 00 00 00 	lea    0x0(,%r15,8),%rcx
   9b993:	00 
   9b994:	48 01 c8             	add    %rcx,%rax
   9b997:	0f 80 64 30 00 00    	jo     9ea01 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5401>
   9b99d:	49 89 46 40          	mov    %rax,0x40(%r14)
   9b9a1:	4d 85 ff             	test   %r15,%r15
   9b9a4:	48 8b 5c 24 08       	mov    0x8(%rsp),%rbx
   9b9a9:	4c 8b 6c 24 20       	mov    0x20(%rsp),%r13
   9b9ae:	7f 33                	jg     9b9e3 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x23e3>
   9b9b0:	e9 1e 01 00 00       	jmp    9bad3 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x24d3>
   9b9b5:	48 89 b4 24 00 02 00 	mov    %rsi,0x200(%rsp)
   9b9bc:	00 
   9b9bd:	48 89 84 24 08 02 00 	mov    %rax,0x208(%rsp)
   9b9c4:	00 
   9b9c5:	48 89 94 24 10 02 00 	mov    %rdx,0x210(%rsp)
   9b9cc:	00 
   9b9cd:	48 8d b4 24 00 02 00 	lea    0x200(%rsp),%rsi
   9b9d4:	00 
   9b9d5:	e8 26 26 0c 00       	call   15e000 <__wolf_rt_list_push>
   9b9da:	49 ff cf             	dec    %r15
   9b9dd:	0f 84 f0 00 00 00    	je     9bad3 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x24d3>
   9b9e3:	4c 89 f7             	mov    %r14,%rdi
   9b9e6:	4c 89 ee             	mov    %r13,%rsi
   9b9e9:	48 89 da             	mov    %rbx,%rdx
   9b9ec:	e8 1f 59 02 00       	call   c1310 <_Wfl.fl_read8$a2ddcd4dfd790b8d>
   9b9f1:	48 89 da             	mov    %rbx,%rdx
   9b9f4:	48 89 c3             	mov    %rax,%rbx
   9b9f7:	4c 89 f7             	mov    %r14,%rdi
   9b9fa:	4c 89 ee             	mov    %r13,%rsi
   9b9fd:	e8 0e 59 02 00       	call   c1310 <_Wfl.fl_read8$a2ddcd4dfd790b8d>
   9ba02:	48 85 db             	test   %rbx,%rbx
   9ba05:	74 42                	je     9ba49 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x2449>
   9ba07:	48 03 44 24 40       	add    0x40(%rsp),%rax
   9ba0c:	0f 80 b0 2e 00 00    	jo     9e8c2 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x52c2>
   9ba12:	4c 89 f7             	mov    %r14,%rdi
   9ba15:	48 89 c6             	mov    %rax,%rsi
   9ba18:	48 8d 9c 24 d8 04 00 	lea    0x4d8(%rsp),%rbx
   9ba1f:	00 
   9ba20:	48 89 da             	mov    %rbx,%rdx
   9ba23:	e8 e8 4f 02 00       	call   c0a10 <_Wfl.fl_capture_upval$a2bcd0b46b228105>
   9ba28:	48 8b 84 24 e0 04 00 	mov    0x4e0(%rsp),%rax
   9ba2f:	00 
   9ba30:	48 8b 94 24 e8 04 00 	mov    0x4e8(%rsp),%rdx
   9ba37:	00 
   9ba38:	49 8b 0e             	mov    (%r14),%rcx
   9ba3b:	48 8b 71 08          	mov    0x8(%rcx),%rsi
   9ba3f:	48 89 df             	mov    %rbx,%rdi
   9ba42:	48 8b 5c 24 08       	mov    0x8(%rsp),%rbx
   9ba47:	eb 43                	jmp    9ba8c <_Wfl.fl_vm_exec$a5081fe92876eb37+0x248c>
   9ba49:	49 8b 0e             	mov    (%r14),%rcx
   9ba4c:	48 8b 71 08          	mov    0x8(%rcx),%rsi
   9ba50:	48 39 74 24 48       	cmp    %rsi,0x48(%rsp)
   9ba55:	0f 83 1c 31 00 00    	jae    9eb77 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5577>
   9ba5b:	48 8b 11             	mov    (%rcx),%rdx
   9ba5e:	48 8b 7c 24 28       	mov    0x28(%rsp),%rdi
   9ba63:	48 8b 54 3a 29       	mov    0x29(%rdx,%rdi,1),%rdx
   9ba68:	48 3b 42 08          	cmp    0x8(%rdx),%rax
   9ba6c:	48 8b 5c 24 08       	mov    0x8(%rsp),%rbx
   9ba71:	0f 83 00 31 00 00    	jae    9eb77 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5577>
   9ba77:	48 8b 12             	mov    (%rdx),%rdx
   9ba7a:	4c 8d 04 40          	lea    (%rax,%rax,2),%r8
   9ba7e:	4a 8d 3c c2          	lea    (%rdx,%r8,8),%rdi
   9ba82:	4a 8b 44 c2 08       	mov    0x8(%rdx,%r8,8),%rax
   9ba87:	4a 8b 54 c2 10       	mov    0x10(%rdx,%r8,8),%rdx
   9ba8c:	49 39 f4             	cmp    %rsi,%r12
   9ba8f:	0f 83 5e 30 00 00    	jae    9eaf3 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x54f3>
   9ba95:	48 8b 37             	mov    (%rdi),%rsi
   9ba98:	48 8b 09             	mov    (%rcx),%rcx
   9ba9b:	48 8b 7c 29 29       	mov    0x29(%rcx,%rbp,1),%rdi
   9baa0:	48 8b 4f 08          	mov    0x8(%rdi),%rcx
   9baa4:	48 3b 4f 10          	cmp    0x10(%rdi),%rcx
   9baa8:	0f 83 07 ff ff ff    	jae    9b9b5 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x23b5>
   9baae:	4c 8b 07             	mov    (%rdi),%r8
   9bab1:	4c 8d 0c 49          	lea    (%rcx,%rcx,2),%r9
   9bab5:	4b 89 34 c8          	mov    %rsi,(%r8,%r9,8)
   9bab9:	4b 89 44 c8 08       	mov    %rax,0x8(%r8,%r9,8)
   9babe:	4b 89 54 c8 10       	mov    %rdx,0x10(%r8,%r9,8)
   9bac3:	48 ff c1             	inc    %rcx
   9bac6:	48 89 4f 08          	mov    %rcx,0x8(%rdi)
   9baca:	49 ff cf             	dec    %r15
   9bacd:	0f 85 10 ff ff ff    	jne    9b9e3 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x23e3>
   9bad3:	4c 89 f7             	mov    %r14,%rdi
   9bad6:	48 8d b4 24 58 02 00 	lea    0x258(%rsp),%rsi
   9badd:	00 
   9bade:	e9 19 e9 ff ff       	jmp    9a3fc <_Wfl.fl_vm_exec$a5081fe92876eb37+0xdfc>
   9bae3:	4c 89 f7             	mov    %r14,%rdi
   9bae6:	4c 89 ee             	mov    %r13,%rsi
   9bae9:	48 8b 54 24 08       	mov    0x8(%rsp),%rdx
   9baee:	e8 cd 58 02 00       	call   c13c0 <_Wfl.fl_read16$7e28ba906e98be2a>
   9baf3:	48 89 c3             	mov    %rax,%rbx
   9baf6:	be 05 00 00 00       	mov    $0x5,%esi
   9bafb:	ba 28 00 00 00       	mov    $0x28,%edx
   9bb00:	4c 89 f7             	mov    %r14,%rdi
   9bb03:	4c 8d a4 24 00 21 00 	lea    0x2100(%rsp),%r12
   9bb0a:	00 
   9bb0b:	4c 89 e1             	mov    %r12,%rcx
   9bb0e:	e8 8d 4b 07 00       	call   1106a0 <_Wfl.fl_gc_alloc$34adf57915ed2b38>
   9bb13:	48 85 db             	test   %rbx,%rbx
   9bb16:	4c 8d ac 24 e0 03 00 	lea    0x3e0(%rsp),%r13
   9bb1d:	00 
   9bb1e:	7e 65                	jle    9bb85 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x2585>
   9bb20:	45 31 ff             	xor    %r15d,%r15d
   9bb23:	49 8b 86 ac 00 00 00 	mov    0xac(%r14),%rax
   9bb2a:	48 29 d8             	sub    %rbx,%rax
   9bb2d:	0f 80 cc 2c 00 00    	jo     9e7ff <_Wfl.fl_vm_exec$a5081fe92876eb37+0x51ff>
   9bb33:	4c 01 f8             	add    %r15,%rax
   9bb36:	0f 80 c3 2c 00 00    	jo     9e7ff <_Wfl.fl_vm_exec$a5081fe92876eb37+0x51ff>
   9bb3c:	49 8b 8e a4 00 00 00 	mov    0xa4(%r14),%rcx
   9bb43:	48 3b 41 08          	cmp    0x8(%rcx),%rax
   9bb47:	0f 83 fe 2f 00 00    	jae    9eb4b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x554b>
   9bb4d:	48 8b 09             	mov    (%rcx),%rcx
   9bb50:	48 8d 04 40          	lea    (%rax,%rax,2),%rax
   9bb54:	f3 0f 7e 44 c1 10    	movq   0x10(%rcx,%rax,8),%xmm0
   9bb5a:	0f 10 0c c1          	movups (%rcx,%rax,8),%xmm1
   9bb5e:	0f 29 8c 24 e0 03 00 	movaps %xmm1,0x3e0(%rsp)
   9bb65:	00 
   9bb66:	66 0f d6 84 24 f0 03 	movq   %xmm0,0x3f0(%rsp)
   9bb6d:	00 00 
   9bb6f:	4c 89 f7             	mov    %r14,%rdi
   9bb72:	4c 89 e6             	mov    %r12,%rsi
   9bb75:	4c 89 ea             	mov    %r13,%rdx
   9bb78:	e8 03 5a 05 00       	call   f1580 <_Wfl.fl_list_push$255051f5e64d80eb>
   9bb7d:	49 ff c7             	inc    %r15
   9bb80:	4c 39 fb             	cmp    %r15,%rbx
   9bb83:	75 9e                	jne    9bb23 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x2523>
   9bb85:	49 8b 86 ac 00 00 00 	mov    0xac(%r14),%rax
   9bb8c:	48 29 d8             	sub    %rbx,%rax
   9bb8f:	0f 80 98 2e 00 00    	jo     9ea2d <_Wfl.fl_vm_exec$a5081fe92876eb37+0x542d>
   9bb95:	49 89 86 ac 00 00 00 	mov    %rax,0xac(%r14)
   9bb9c:	4c 89 f7             	mov    %r14,%rdi
   9bb9f:	4c 89 e6             	mov    %r12,%rsi
   9bba2:	e9 25 e9 ff ff       	jmp    9a4cc <_Wfl.fl_vm_exec$a5081fe92876eb37+0xecc>
   9bba7:	48 83 f8 2c          	cmp    $0x2c,%rax
   9bbab:	0f 85 9b 02 00 00    	jne    9be4c <_Wfl.fl_vm_exec$a5081fe92876eb37+0x284c>
   9bbb1:	4c 8b 74 24 10       	mov    0x10(%rsp),%r14
   9bbb6:	4c 89 f7             	mov    %r14,%rdi
   9bbb9:	4c 89 ee             	mov    %r13,%rsi
   9bbbc:	48 8b 5c 24 08       	mov    0x8(%rsp),%rbx
   9bbc1:	48 89 da             	mov    %rbx,%rdx
   9bbc4:	e8 f7 57 02 00       	call   c13c0 <_Wfl.fl_read16$7e28ba906e98be2a>
   9bbc9:	49 8b 0e             	mov    (%r14),%rcx
   9bbcc:	48 3b 59 08          	cmp    0x8(%rcx),%rbx
   9bbd0:	0f 83 43 31 00 00    	jae    9ed19 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5719>
   9bbd6:	48 8b 09             	mov    (%rcx),%rcx
   9bbd9:	48 69 54 24 08 60 01 	imul   $0x160,0x8(%rsp),%rdx
   9bbe0:	00 00 
   9bbe2:	48 8b 4c 11 29       	mov    0x29(%rcx,%rdx,1),%rcx
   9bbe7:	48 3b 41 08          	cmp    0x8(%rcx),%rax
   9bbeb:	0f 83 28 31 00 00    	jae    9ed19 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5719>
   9bbf1:	48 8b 09             	mov    (%rcx),%rcx
   9bbf4:	48 8d 04 40          	lea    (%rax,%rax,2),%rax
   9bbf8:	48 8b 5c c1 08       	mov    0x8(%rcx,%rax,8),%rbx
   9bbfd:	48 83 7c 24 20 2d    	cmpq   $0x2d,0x20(%rsp)
   9bc03:	0f 85 cf 03 00 00    	jne    9bfd8 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x29d8>
   9bc09:	48 8b 7c 24 10       	mov    0x10(%rsp),%rdi
   9bc0e:	4c 8d bc 24 b0 05 00 	lea    0x5b0(%rsp),%r15
   9bc15:	00 
   9bc16:	4c 89 fe             	mov    %r15,%rsi
   9bc19:	e8 f2 29 01 00       	call   ae610 <_Wfl.fl_pop$add7471f880d5ef3>
   9bc1e:	f3 0f 6f 84 24 b8 05 	movdqu 0x5b8(%rsp),%xmm0
   9bc25:	00 00 
   9bc27:	e9 b7 03 00 00       	jmp    9bfe3 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x29e3>
   9bc2c:	e8 ef c8 0c 00       	call   168520 <__wolf_rt_strbuf_new>
   9bc31:	48 89 c5             	mov    %rax,%rbp
   9bc34:	4c 89 ef             	mov    %r13,%rdi
   9bc37:	e8 c4 a6 04 00       	call   e6300 <_Wfl.fl_type_name$425de879c2cc2a41>
   9bc3c:	48 89 ef             	mov    %rbp,%rdi
   9bc3f:	48 89 c6             	mov    %rax,%rsi
   9bc42:	31 c9                	xor    %ecx,%ecx
   9bc44:	e8 87 c9 0c 00       	call   1685d0 <__wolf_rt_strbuf_str>
   9bc49:	ba 17 00 00 00       	mov    $0x17,%edx
   9bc4e:	48 89 ef             	mov    %rbp,%rdi
   9bc51:	48 8d 35 2e 8d f7 ff 	lea    -0x872d2(%rip),%rsi        # 14986 <__abi_tag+0x1468a>
   9bc58:	31 c9                	xor    %ecx,%ecx
   9bc5a:	e8 71 c9 0c 00       	call   1685d0 <__wolf_rt_strbuf_str>
   9bc5f:	48 89 ef             	mov    %rbp,%rdi
   9bc62:	48 8d b4 24 c0 01 00 	lea    0x1c0(%rsp),%rsi
   9bc69:	00 
   9bc6a:	e8 61 c5 0c 00       	call   1681d0 <__wolf_rt_strbuf_finish>
   9bc6f:	48 8b 8c 24 c0 01 00 	mov    0x1c0(%rsp),%rcx
   9bc76:	00 
   9bc77:	4c 8b 84 24 c8 01 00 	mov    0x1c8(%rsp),%r8
   9bc7e:	00 
   9bc7f:	ba 03 00 00 00       	mov    $0x3,%edx
   9bc84:	4c 89 f7             	mov    %r14,%rdi
   9bc87:	48 8d 35 f5 8c f7 ff 	lea    -0x8730b(%rip),%rsi        # 14983 <__abi_tag+0x14687>
   9bc8e:	4c 8d 8c 24 e8 20 00 	lea    0x20e8(%rsp),%r9
   9bc95:	00 
   9bc96:	e8 65 2b 07 00       	call   10e800 <_Wfl.fl_raise_bytes.bytesview.4$e8b5fa06fc8adc2d>
   9bc9b:	41 80 be 5c 01 00 00 	cmpb   $0x0,0x15c(%r14)
   9bca2:	00 
   9bca3:	0f 85 d4 f2 ff ff    	jne    9af7d <_Wfl.fl_vm_exec$a5081fe92876eb37+0x197d>
   9bca9:	48 8b 44 24 28       	mov    0x28(%rsp),%rax
   9bcae:	48 b9 00 00 00 00 00 	movabs $0x4000000000000000,%rcx
   9bcb5:	00 00 40 
   9bcb8:	48 01 c8             	add    %rcx,%rax
   9bcbb:	0f 88 c4 2d 00 00    	js     9ea85 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5485>
   9bcc1:	48 8b 44 24 10       	mov    0x10(%rsp),%rax
   9bcc6:	48 8b 80 ac 00 00 00 	mov    0xac(%rax),%rax
   9bccd:	48 8b 4c 24 28       	mov    0x28(%rsp),%rcx
   9bcd2:	48 01 c9             	add    %rcx,%rcx
   9bcd5:	48 29 c8             	sub    %rcx,%rax
   9bcd8:	0f 80 a7 2d 00 00    	jo     9ea85 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5485>
   9bcde:	4c 8b 74 24 10       	mov    0x10(%rsp),%r14
   9bce3:	49 89 86 ac 00 00 00 	mov    %rax,0xac(%r14)
   9bcea:	4c 89 f7             	mov    %r14,%rdi
   9bced:	48 8d b4 24 c0 04 00 	lea    0x4c0(%rsp),%rsi
   9bcf4:	00 
   9bcf5:	e9 d2 e7 ff ff       	jmp    9a4cc <_Wfl.fl_vm_exec$a5081fe92876eb37+0xecc>
   9bcfa:	f3 0f 6f 44 24 58    	movdqu 0x58(%rsp),%xmm0
   9bd00:	48 8d 5c 24 50       	lea    0x50(%rsp),%rbx
   9bd05:	48 8b 03             	mov    (%rbx),%rax
   9bd08:	48 89 84 24 d0 02 00 	mov    %rax,0x2d0(%rsp)
   9bd0f:	00 
   9bd10:	f3 0f 7f 84 24 d8 02 	movdqu %xmm0,0x2d8(%rsp)
   9bd17:	00 00 
   9bd19:	48 8b 5c 24 10       	mov    0x10(%rsp),%rbx
   9bd1e:	48 89 df             	mov    %rbx,%rdi
   9bd21:	48 8d 74 24 70       	lea    0x70(%rsp),%rsi
   9bd26:	e8 e5 28 01 00       	call   ae610 <_Wfl.fl_pop$add7471f880d5ef3>
   9bd2b:	48 89 df             	mov    %rbx,%rdi
   9bd2e:	48 8d b4 24 18 01 00 	lea    0x118(%rsp),%rsi
   9bd35:	00 
   9bd36:	e8 d5 28 01 00       	call   ae610 <_Wfl.fl_pop$add7471f880d5ef3>
   9bd3b:	4c 8b ac 24 18 01 00 	mov    0x118(%rsp),%r13
   9bd42:	00 
   9bd43:	49 83 fd 06          	cmp    $0x6,%r13
   9bd47:	0f 84 83 01 00 00    	je     9bed0 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x28d0>
   9bd4d:	49 83 fd 05          	cmp    $0x5,%r13
   9bd51:	0f 85 22 02 00 00    	jne    9bf79 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x2979>
   9bd57:	48 83 7c 24 20 2a    	cmpq   $0x2a,0x20(%rsp)
   9bd5d:	0f 95 c1             	setne  %cl
   9bd60:	48 8b 44 24 70       	mov    0x70(%rsp),%rax
   9bd65:	48 83 f8 02          	cmp    $0x2,%rax
   9bd69:	0f 95 c2             	setne  %dl
   9bd6c:	84 d1                	test   %dl,%cl
   9bd6e:	0f 85 11 02 00 00    	jne    9bf85 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x2985>
   9bd74:	48 8b 8c 24 20 01 00 	mov    0x120(%rsp),%rcx
   9bd7b:	00 
   9bd7c:	48 85 c9             	test   %rcx,%rcx
   9bd7f:	0f 88 12 31 00 00    	js     9ee97 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5897>
   9bd85:	48 8b 54 24 10       	mov    0x10(%rsp),%rdx
   9bd8a:	48 8b 12             	mov    (%rdx),%rdx
   9bd8d:	89 ce                	mov    %ecx,%esi
   9bd8f:	48 3b 72 08          	cmp    0x8(%rdx),%rsi
   9bd93:	0f 8d fe 30 00 00    	jge    9ee97 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5897>
   9bd99:	48 8b 12             	mov    (%rdx),%rdx
   9bd9c:	48 69 f6 60 01 00 00 	imul   $0x160,%rsi,%rsi
   9bda3:	48 83 3c 32 05       	cmpq   $0x5,(%rdx,%rsi,1)
   9bda8:	0f 85 e9 30 00 00    	jne    9ee97 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5897>
   9bdae:	48 c1 e9 20          	shr    $0x20,%rcx
   9bdb2:	48 39 4c 32 19       	cmp    %rcx,0x19(%rdx,%rsi,1)
   9bdb7:	0f 85 da 30 00 00    	jne    9ee97 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5897>
   9bdbd:	48 83 f8 02          	cmp    $0x2,%rax
   9bdc1:	0f 85 50 12 00 00    	jne    9d017 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x3a17>
   9bdc7:	48 8b 44 32 29       	mov    0x29(%rdx,%rsi,1),%rax
   9bdcc:	4c 8b 70 08          	mov    0x8(%rax),%r14
   9bdd0:	4c 8b 6c 24 78       	mov    0x78(%rsp),%r13
   9bdd5:	4d 85 ed             	test   %r13,%r13
   9bdd8:	0f 88 8c 12 00 00    	js     9d06a <_Wfl.fl_vm_exec$a5081fe92876eb37+0x3a6a>
   9bdde:	4d 39 f5             	cmp    %r14,%r13
   9bde1:	0f 8d 83 12 00 00    	jge    9d06a <_Wfl.fl_vm_exec$a5081fe92876eb37+0x3a6a>
   9bde7:	48 83 7c 24 20 2a    	cmpq   $0x2a,0x20(%rsp)
   9bded:	0f 84 05 1c 00 00    	je     9d9f8 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x43f8>
   9bdf3:	48 8b 7c 24 10       	mov    0x10(%rsp),%rdi
   9bdf8:	48 8d b4 24 18 01 00 	lea    0x118(%rsp),%rsi
   9bdff:	00 
   9be00:	4c 89 ea             	mov    %r13,%rdx
   9be03:	48 8d 8c 24 d0 02 00 	lea    0x2d0(%rsp),%rcx
   9be0a:	00 
   9be0b:	e8 f0 a5 04 00       	call   e6400 <_Wfl.fl_list_set$6ba3fca6afac6ce9>
   9be10:	84 c0                	test   %al,%al
   9be12:	0f 85 12 27 00 00    	jne    9e52a <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4f2a>
   9be18:	ba 0c 00 00 00       	mov    $0xc,%edx
   9be1d:	4c 8b 74 24 10       	mov    0x10(%rsp),%r14
   9be22:	4c 89 f7             	mov    %r14,%rdi
   9be25:	48 8d 35 33 8a f7 ff 	lea    -0x875cd(%rip),%rsi        # 1485f <__abi_tag+0x14563>
   9be2c:	48 8b 5c 24 08       	mov    0x8(%rsp),%rbx
   9be31:	48 89 d9             	mov    %rbx,%rcx
   9be34:	4c 8b 44 24 28       	mov    0x28(%rsp),%r8
   9be39:	4c 8b 4c 24 20       	mov    0x20(%rsp),%r9
   9be3e:	6a 24                	push   $0x24
   9be40:	48 8d 05 8d 8b f7 ff 	lea    -0x87473(%rip),%rax        # 149d4 <__abi_tag+0x146d8>
   9be47:	e9 ba 19 00 00       	jmp    9d806 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4206>
   9be4c:	48 8b 44 24 20       	mov    0x20(%rsp),%rax
   9be51:	48 83 c0 d2          	add    $0xffffffffffffffd2,%rax
   9be55:	48 83 f8 0d          	cmp    $0xd,%rax
   9be59:	0f 87 dc 15 00 00    	ja     9d43b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x3e3b>
   9be5f:	48 8d 0d fa 82 f7 ff 	lea    -0x87d06(%rip),%rcx        # 14160 <__abi_tag+0x13e64>
   9be66:	48 63 04 81          	movslq (%rcx,%rax,4),%rax
   9be6a:	48 01 c8             	add    %rcx,%rax
   9be6d:	ff e0                	jmp    *%rax
   9be6f:	4c 8b 74 24 10       	mov    0x10(%rsp),%r14
   9be74:	4c 89 f7             	mov    %r14,%rdi
   9be77:	4c 89 ee             	mov    %r13,%rsi
   9be7a:	48 8b 5c 24 08       	mov    0x8(%rsp),%rbx
   9be7f:	48 89 da             	mov    %rbx,%rdx
   9be82:	e8 89 54 02 00       	call   c1310 <_Wfl.fl_read8$a2ddcd4dfd790b8d>
   9be87:	48 89 c5             	mov    %rax,%rbp
   9be8a:	4c 89 f7             	mov    %r14,%rdi
   9be8d:	4c 89 ee             	mov    %r13,%rsi
   9be90:	48 89 da             	mov    %rbx,%rdx
   9be93:	e8 28 55 02 00       	call   c13c0 <_Wfl.fl_read16$7e28ba906e98be2a>
   9be98:	48 03 6c 24 40       	add    0x40(%rsp),%rbp
   9be9d:	0f 80 7c 2c 00 00    	jo     9eb1f <_Wfl.fl_vm_exec$a5081fe92876eb37+0x551f>
   9bea3:	45 31 c0             	xor    %r8d,%r8d
   9bea6:	48 83 7c 24 20 30    	cmpq   $0x30,0x20(%rsp)
   9beac:	41 0f 94 c0          	sete   %r8b
   9beb0:	4c 8b 74 24 10       	mov    0x10(%rsp),%r14
   9beb5:	4c 89 f7             	mov    %r14,%rdi
   9beb8:	4c 89 ee             	mov    %r13,%rsi
   9bebb:	48 89 ea             	mov    %rbp,%rdx
   9bebe:	48 89 c1             	mov    %rax,%rcx
   9bec1:	e8 8a ee 06 00       	call   10ad50 <_Wfl.fl_iter_next$312c49f98a86acc1>
   9bec6:	48 8b 6c 24 18       	mov    0x18(%rsp),%rbp
   9becb:	e9 50 f1 ff ff       	jmp    9b020 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1a20>
   9bed0:	48 83 7c 24 20 2a    	cmpq   $0x2a,0x20(%rsp)
   9bed6:	0f 84 82 02 00 00    	je     9c15e <_Wfl.fl_vm_exec$a5081fe92876eb37+0x2b5e>
   9bedc:	48 8b 7c 24 10       	mov    0x10(%rsp),%rdi
   9bee1:	48 8d b4 24 18 01 00 	lea    0x118(%rsp),%rsi
   9bee8:	00 
   9bee9:	48 8d 54 24 70       	lea    0x70(%rsp),%rdx
   9beee:	48 8d 8c 24 d0 02 00 	lea    0x2d0(%rsp),%rcx
   9bef5:	00 
   9bef6:	e8 f5 76 ff ff       	call   935f0 <_Wfl.fl_map_set$540fcc92758450a0>
   9befb:	84 c0                	test   %al,%al
   9befd:	0f 85 27 26 00 00    	jne    9e52a <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4f2a>
   9bf03:	e8 18 c6 0c 00       	call   168520 <__wolf_rt_strbuf_new>
   9bf08:	48 89 c3             	mov    %rax,%rbx
   9bf0b:	48 8b 7c 24 70       	mov    0x70(%rsp),%rdi
   9bf10:	e8 eb a3 04 00       	call   e6300 <_Wfl.fl_type_name$425de879c2cc2a41>
   9bf15:	48 89 df             	mov    %rbx,%rdi
   9bf18:	48 89 c6             	mov    %rax,%rsi
   9bf1b:	31 c9                	xor    %ecx,%ecx
   9bf1d:	e8 ae c6 0c 00       	call   1685d0 <__wolf_rt_strbuf_str>
   9bf22:	ba 17 00 00 00       	mov    $0x17,%edx
   9bf27:	48 89 df             	mov    %rbx,%rdi
   9bf2a:	48 8d 35 55 8a f7 ff 	lea    -0x875ab(%rip),%rsi        # 14986 <__abi_tag+0x1468a>
   9bf31:	31 c9                	xor    %ecx,%ecx
   9bf33:	e8 98 c6 0c 00       	call   1685d0 <__wolf_rt_strbuf_str>
   9bf38:	48 89 df             	mov    %rbx,%rdi
   9bf3b:	48 8d b4 24 a0 01 00 	lea    0x1a0(%rsp),%rsi
   9bf42:	00 
   9bf43:	e8 88 c2 0c 00       	call   1681d0 <__wolf_rt_strbuf_finish>
   9bf48:	48 8b 8c 24 a0 01 00 	mov    0x1a0(%rsp),%rcx
   9bf4f:	00 
   9bf50:	4c 8b 84 24 a8 01 00 	mov    0x1a8(%rsp),%r8
   9bf57:	00 
   9bf58:	ba 03 00 00 00       	mov    $0x3,%edx
   9bf5d:	4c 8b 74 24 10       	mov    0x10(%rsp),%r14
   9bf62:	4c 89 f7             	mov    %r14,%rdi
   9bf65:	48 8d 35 17 8a f7 ff 	lea    -0x875e9(%rip),%rsi        # 14983 <__abi_tag+0x14687>
   9bf6c:	4c 8d 8c 24 88 20 00 	lea    0x2088(%rsp),%r9
   9bf73:	00 
   9bf74:	e9 96 08 00 00       	jmp    9c80f <_Wfl.fl_vm_exec$a5081fe92876eb37+0x320f>
   9bf79:	48 83 7c 24 20 2a    	cmpq   $0x2a,0x20(%rsp)
   9bf7f:	0f 84 1d 08 00 00    	je     9c7a2 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x31a2>
   9bf85:	e8 96 c5 0c 00       	call   168520 <__wolf_rt_strbuf_new>
   9bf8a:	48 89 c3             	mov    %rax,%rbx
   9bf8d:	ba 13 00 00 00       	mov    $0x13,%edx
   9bf92:	48 89 c7             	mov    %rax,%rdi
   9bf95:	48 8d 35 74 8a f7 ff 	lea    -0x8758c(%rip),%rsi        # 14a10 <__abi_tag+0x14714>
   9bf9c:	31 c9                	xor    %ecx,%ecx
   9bf9e:	e8 2d c6 0c 00       	call   1685d0 <__wolf_rt_strbuf_str>
   9bfa3:	4c 89 ef             	mov    %r13,%rdi
   9bfa6:	e8 55 a3 04 00       	call   e6300 <_Wfl.fl_type_name$425de879c2cc2a41>
   9bfab:	48 89 df             	mov    %rbx,%rdi
   9bfae:	48 89 c6             	mov    %rax,%rsi
   9bfb1:	31 c9                	xor    %ecx,%ecx
   9bfb3:	e8 18 c6 0c 00       	call   1685d0 <__wolf_rt_strbuf_str>
   9bfb8:	48 89 df             	mov    %rbx,%rdi
   9bfbb:	48 8d 9c 24 80 03 00 	lea    0x380(%rsp),%rbx
   9bfc2:	00 
   9bfc3:	48 89 de             	mov    %rbx,%rsi
   9bfc6:	e8 05 c2 0c 00       	call   1681d0 <__wolf_rt_strbuf_finish>
   9bfcb:	4c 8b 84 24 88 03 00 	mov    0x388(%rsp),%r8
   9bfd2:	00 
   9bfd3:	e9 18 08 00 00       	jmp    9c7f0 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x31f0>
   9bfd8:	f3 0f 6f 44 24 58    	movdqu 0x58(%rsp),%xmm0
   9bfde:	4c 8d 7c 24 50       	lea    0x50(%rsp),%r15
   9bfe3:	49 8b 07             	mov    (%r15),%rax
   9bfe6:	48 89 84 24 38 05 00 	mov    %rax,0x538(%rsp)
   9bfed:	00 
   9bfee:	f3 0f 7f 84 24 40 05 	movdqu %xmm0,0x540(%rsp)
   9bff5:	00 00 
   9bff7:	4c 8b 74 24 10       	mov    0x10(%rsp),%r14
   9bffc:	4c 89 f7             	mov    %r14,%rdi
   9bfff:	48 8d b4 24 b8 00 00 	lea    0xb8(%rsp),%rsi
   9c006:	00 
   9c007:	e8 04 26 01 00       	call   ae610 <_Wfl.fl_pop$add7471f880d5ef3>
   9c00c:	49 8b 46 6a          	mov    0x6a(%r14),%rax
   9c010:	48 3b 58 08          	cmp    0x8(%rax),%rbx
   9c014:	0f 83 23 2c 00 00    	jae    9ec3d <_Wfl.fl_vm_exec$a5081fe92876eb37+0x563d>
   9c01a:	48 8b 00             	mov    (%rax),%rax
   9c01d:	48 8b 3c d8          	mov    (%rax,%rbx,8),%rdi
   9c021:	48 8d b4 24 90 01 00 	lea    0x190(%rsp),%rsi
   9c028:	00 
   9c029:	e8 32 bd 0c 00       	call   167d60 <__wolf_rt_str_from_utf8>
   9c02e:	48 85 c0             	test   %rax,%rax
   9c031:	0f 85 71 1b 00 00    	jne    9dba8 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x45a8>
   9c037:	4c 8b ac 24 90 01 00 	mov    0x190(%rsp),%r13
   9c03e:	00 
   9c03f:	48 8b ac 24 98 01 00 	mov    0x198(%rsp),%rbp
   9c046:	00 
   9c047:	48 83 7c 24 20 2c    	cmpq   $0x2c,0x20(%rsp)
   9c04d:	0f 94 c1             	sete   %cl
   9c050:	48 8b 84 24 b8 00 00 	mov    0xb8(%rsp),%rax
   9c057:	00 
   9c058:	48 83 f8 06          	cmp    $0x6,%rax
   9c05c:	0f 95 c2             	setne  %dl
   9c05f:	20 ca                	and    %cl,%dl
   9c061:	80 fa 01             	cmp    $0x1,%dl
   9c064:	75 64                	jne    9c0ca <_Wfl.fl_vm_exec$a5081fe92876eb37+0x2aca>
   9c066:	48 8b 7c 24 10       	mov    0x10(%rsp),%rdi
   9c06b:	48 8d b4 24 b8 00 00 	lea    0xb8(%rsp),%rsi
   9c072:	00 
   9c073:	4c 89 ea             	mov    %r13,%rdx
   9c076:	48 89 e9             	mov    %rbp,%rcx
   9c079:	4c 8d 84 24 a0 04 00 	lea    0x4a0(%rsp),%r8
   9c080:	00 
   9c081:	e8 0a 57 08 00       	call   121790 <_Wfl.fl_api_bind_receiver.bytesview.4$fea5acaf256156a6>
   9c086:	48 83 bc 24 a0 04 00 	cmpq   $0x0,0x4a0(%rsp)
   9c08d:	00 00 
   9c08f:	0f 85 24 1b 00 00    	jne    9dbb9 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x45b9>
   9c095:	f3 0f 6f 84 24 a8 04 	movdqu 0x4a8(%rsp),%xmm0
   9c09c:	00 00 
   9c09e:	48 8b 84 24 b8 04 00 	mov    0x4b8(%rsp),%rax
   9c0a5:	00 
   9c0a6:	66 0f 7f 84 24 60 04 	movdqa %xmm0,0x460(%rsp)
   9c0ad:	00 00 
   9c0af:	48 89 84 24 70 04 00 	mov    %rax,0x470(%rsp)
   9c0b6:	00 
   9c0b7:	66 48 0f 7e c0       	movq   %xmm0,%rax
   9c0bc:	48 85 c0             	test   %rax,%rax
   9c0bf:	0f 85 2d d6 ff ff    	jne    996f2 <_Wfl.fl_vm_exec$a5081fe92876eb37+0xf2>
   9c0c5:	e9 33 08 00 00       	jmp    9c8fd <_Wfl.fl_vm_exec$a5081fe92876eb37+0x32fd>
   9c0ca:	48 83 f8 06          	cmp    $0x6,%rax
   9c0ce:	0f 95 c1             	setne  %cl
   9c0d1:	48 83 7c 24 20 2d    	cmpq   $0x2d,0x20(%rsp)
   9c0d7:	0f 94 c2             	sete   %dl
   9c0da:	20 ca                	and    %cl,%dl
   9c0dc:	80 fa 01             	cmp    $0x1,%dl
   9c0df:	0f 85 39 07 00 00    	jne    9c81e <_Wfl.fl_vm_exec$a5081fe92876eb37+0x321e>
   9c0e5:	e8 36 c4 0c 00       	call   168520 <__wolf_rt_strbuf_new>
   9c0ea:	48 89 c3             	mov    %rax,%rbx
   9c0ed:	ba 16 00 00 00       	mov    $0x16,%edx
   9c0f2:	48 89 c7             	mov    %rax,%rdi
   9c0f5:	48 8d 35 27 89 f7 ff 	lea    -0x876d9(%rip),%rsi        # 14a23 <__abi_tag+0x14727>
   9c0fc:	31 c9                	xor    %ecx,%ecx
   9c0fe:	e8 cd c4 0c 00       	call   1685d0 <__wolf_rt_strbuf_str>
   9c103:	48 8b bc 24 b8 00 00 	mov    0xb8(%rsp),%rdi
   9c10a:	00 
   9c10b:	e8 f0 a1 04 00       	call   e6300 <_Wfl.fl_type_name$425de879c2cc2a41>
   9c110:	48 89 df             	mov    %rbx,%rdi
   9c113:	48 89 c6             	mov    %rax,%rsi
   9c116:	31 c9                	xor    %ecx,%ecx
   9c118:	e8 b3 c4 0c 00       	call   1685d0 <__wolf_rt_strbuf_str>
   9c11d:	48 89 df             	mov    %rbx,%rdi
   9c120:	48 8d b4 24 80 01 00 	lea    0x180(%rsp),%rsi
   9c127:	00 
   9c128:	e8 a3 c0 0c 00       	call   1681d0 <__wolf_rt_strbuf_finish>
   9c12d:	48 8b 8c 24 80 01 00 	mov    0x180(%rsp),%rcx
   9c134:	00 
   9c135:	4c 8b 84 24 88 01 00 	mov    0x188(%rsp),%r8
   9c13c:	00 
   9c13d:	ba 04 00 00 00       	mov    $0x4,%edx
   9c142:	4c 8b 74 24 10       	mov    0x10(%rsp),%r14
   9c147:	4c 89 f7             	mov    %r14,%rdi
   9c14a:	48 8d 35 8f 2f f8 ff 	lea    -0x7d071(%rip),%rsi        # 1f0e0 <__abi_tag+0x1ede4>
   9c151:	4c 8d 8c 24 58 20 00 	lea    0x2058(%rsp),%r9
   9c158:	00 
   9c159:	e9 b1 06 00 00       	jmp    9c80f <_Wfl.fl_vm_exec$a5081fe92876eb37+0x320f>
   9c15e:	48 8b 54 24 10       	mov    0x10(%rsp),%rdx
   9c163:	48 8b 4a 20          	mov    0x20(%rdx),%rcx
   9c167:	48 8b 42 48          	mov    0x48(%rdx),%rax
   9c16b:	0f 10 02             	movups (%rdx),%xmm0
   9c16e:	0f 29 84 24 70 10 00 	movaps %xmm0,0x1070(%rsp)
   9c175:	00 
   9c176:	0f 10 42 10          	movups 0x10(%rdx),%xmm0
   9c17a:	0f 29 84 24 80 10 00 	movaps %xmm0,0x1080(%rsp)
   9c181:	00 
   9c182:	48 89 8c 24 90 10 00 	mov    %rcx,0x1090(%rsp)
   9c189:	00 
   9c18a:	0f b7 4a 50          	movzwl 0x50(%rdx),%ecx
   9c18e:	0f 10 42 28          	movups 0x28(%rdx),%xmm0
   9c192:	0f 11 84 24 98 10 00 	movups %xmm0,0x1098(%rsp)
   9c199:	00 
   9c19a:	0f 10 42 38          	movups 0x38(%rdx),%xmm0
   9c19e:	0f 11 84 24 a8 10 00 	movups %xmm0,0x10a8(%rsp)
   9c1a5:	00 
   9c1a6:	48 89 84 24 b8 10 00 	mov    %rax,0x10b8(%rsp)
   9c1ad:	00 
   9c1ae:	48 8b 42 62          	mov    0x62(%rdx),%rax
   9c1b2:	48 89 8c 24 c0 10 00 	mov    %rcx,0x10c0(%rsp)
   9c1b9:	00 
   9c1ba:	48 8b 8a 8a 00 00 00 	mov    0x8a(%rdx),%rcx
   9c1c1:	0f 10 42 52          	movups 0x52(%rdx),%xmm0
   9c1c5:	0f 11 84 24 c8 10 00 	movups %xmm0,0x10c8(%rsp)
   9c1cc:	00 
   9c1cd:	48 89 84 24 d8 10 00 	mov    %rax,0x10d8(%rsp)
   9c1d4:	00 
   9c1d5:	0f b7 82 a2 00 00 00 	movzwl 0xa2(%rdx),%eax
   9c1dc:	0f 10 42 6a          	movups 0x6a(%rdx),%xmm0
   9c1e0:	0f 29 84 24 e0 10 00 	movaps %xmm0,0x10e0(%rsp)
   9c1e7:	00 
   9c1e8:	0f 10 42 7a          	movups 0x7a(%rdx),%xmm0
   9c1ec:	0f 29 84 24 f0 10 00 	movaps %xmm0,0x10f0(%rsp)
   9c1f3:	00 
   9c1f4:	0f 10 82 92 00 00 00 	movups 0x92(%rdx),%xmm0
   9c1fb:	48 89 8c 24 00 11 00 	mov    %rcx,0x1100(%rsp)
   9c202:	00 
   9c203:	0f 11 84 24 08 11 00 	movups %xmm0,0x1108(%rsp)
   9c20a:	00 
   9c20b:	0f 10 82 a4 00 00 00 	movups 0xa4(%rdx),%xmm0
   9c212:	48 8b 8a c4 00 00 00 	mov    0xc4(%rdx),%rcx
   9c219:	48 89 84 24 18 11 00 	mov    %rax,0x1118(%rsp)
   9c220:	00 
   9c221:	0f b6 82 5c 01 00 00 	movzbl 0x15c(%rdx),%eax
   9c228:	0f 29 84 24 20 11 00 	movaps %xmm0,0x1120(%rsp)
   9c22f:	00 
   9c230:	0f 10 82 b4 00 00 00 	movups 0xb4(%rdx),%xmm0
   9c237:	0f 29 84 24 30 11 00 	movaps %xmm0,0x1130(%rsp)
   9c23e:	00 
   9c23f:	0f 10 82 cc 00 00 00 	movups 0xcc(%rdx),%xmm0
   9c246:	48 89 8c 24 40 11 00 	mov    %rcx,0x1140(%rsp)
   9c24d:	00 
   9c24e:	0f 11 84 24 48 11 00 	movups %xmm0,0x1148(%rsp)
   9c255:	00 
   9c256:	0f 10 82 dc 00 00 00 	movups 0xdc(%rdx),%xmm0
   9c25d:	0f 11 84 24 58 11 00 	movups %xmm0,0x1158(%rsp)
   9c264:	00 
   9c265:	0f 10 82 ec 00 00 00 	movups 0xec(%rdx),%xmm0
   9c26c:	0f 11 84 24 68 11 00 	movups %xmm0,0x1168(%rsp)
   9c273:	00 
   9c274:	0f 10 82 fc 00 00 00 	movups 0xfc(%rdx),%xmm0
   9c27b:	0f 11 84 24 78 11 00 	movups %xmm0,0x1178(%rsp)
   9c282:	00 
   9c283:	0f 10 82 0c 01 00 00 	movups 0x10c(%rdx),%xmm0
   9c28a:	0f 11 84 24 88 11 00 	movups %xmm0,0x1188(%rsp)
   9c291:	00 
   9c292:	0f 10 82 1c 01 00 00 	movups 0x11c(%rdx),%xmm0
   9c299:	0f 11 84 24 98 11 00 	movups %xmm0,0x1198(%rsp)
   9c2a0:	00 
   9c2a1:	0f 10 82 2c 01 00 00 	movups 0x12c(%rdx),%xmm0
   9c2a8:	0f 11 84 24 a8 11 00 	movups %xmm0,0x11a8(%rsp)
   9c2af:	00 
   9c2b0:	0f 10 82 3c 01 00 00 	movups 0x13c(%rdx),%xmm0
   9c2b7:	0f 11 84 24 b8 11 00 	movups %xmm0,0x11b8(%rsp)
   9c2be:	00 
   9c2bf:	0f 10 82 4c 01 00 00 	movups 0x14c(%rdx),%xmm0
   9c2c6:	0f 11 84 24 c8 11 00 	movups %xmm0,0x11c8(%rsp)
   9c2cd:	00 
   9c2ce:	0f 10 82 5d 01 00 00 	movups 0x15d(%rdx),%xmm0
   9c2d5:	88 84 24 d8 11 00 00 	mov    %al,0x11d8(%rsp)
   9c2dc:	0f 29 84 24 e0 11 00 	movaps %xmm0,0x11e0(%rsp)
   9c2e3:	00 
   9c2e4:	0f 10 82 6d 01 00 00 	movups 0x16d(%rdx),%xmm0
   9c2eb:	0f b6 82 dd 01 00 00 	movzbl 0x1dd(%rdx),%eax
   9c2f2:	48 8b 8a ee 01 00 00 	mov    0x1ee(%rdx),%rcx
   9c2f9:	0f 29 84 24 f0 11 00 	movaps %xmm0,0x11f0(%rsp)
   9c300:	00 
   9c301:	0f 10 82 7d 01 00 00 	movups 0x17d(%rdx),%xmm0
   9c308:	0f 29 84 24 00 12 00 	movaps %xmm0,0x1200(%rsp)
   9c30f:	00 
   9c310:	0f 10 82 8d 01 00 00 	movups 0x18d(%rdx),%xmm0
   9c317:	0f 29 84 24 10 12 00 	movaps %xmm0,0x1210(%rsp)
   9c31e:	00 
   9c31f:	0f 10 82 9d 01 00 00 	movups 0x19d(%rdx),%xmm0
   9c326:	0f 29 84 24 20 12 00 	movaps %xmm0,0x1220(%rsp)
   9c32d:	00 
   9c32e:	0f 10 82 ad 01 00 00 	movups 0x1ad(%rdx),%xmm0
   9c335:	0f 29 84 24 30 12 00 	movaps %xmm0,0x1230(%rsp)
   9c33c:	00 
   9c33d:	0f 10 82 bd 01 00 00 	movups 0x1bd(%rdx),%xmm0
   9c344:	0f 29 84 24 40 12 00 	movaps %xmm0,0x1240(%rsp)
   9c34b:	00 
   9c34c:	0f 10 82 cd 01 00 00 	movups 0x1cd(%rdx),%xmm0
   9c353:	0f 29 84 24 50 12 00 	movaps %xmm0,0x1250(%rsp)
   9c35a:	00 
   9c35b:	88 84 24 60 12 00 00 	mov    %al,0x1260(%rsp)
   9c362:	48 8b 82 16 02 00 00 	mov    0x216(%rdx),%rax
   9c369:	0f 10 82 de 01 00 00 	movups 0x1de(%rdx),%xmm0
   9c370:	0f 11 84 24 68 12 00 	movups %xmm0,0x1268(%rsp)
   9c377:	00 
   9c378:	48 89 8c 24 78 12 00 	mov    %rcx,0x1278(%rsp)
   9c37f:	00 
   9c380:	0f b6 8a 1e 02 00 00 	movzbl 0x21e(%rdx),%ecx
   9c387:	0f 10 82 f6 01 00 00 	movups 0x1f6(%rdx),%xmm0
   9c38e:	0f 29 84 24 80 12 00 	movaps %xmm0,0x1280(%rsp)
   9c395:	00 
   9c396:	0f 10 82 06 02 00 00 	movups 0x206(%rdx),%xmm0
   9c39d:	0f 29 84 24 90 12 00 	movaps %xmm0,0x1290(%rsp)
   9c3a4:	00 
   9c3a5:	48 89 84 24 a0 12 00 	mov    %rax,0x12a0(%rsp)
   9c3ac:	00 
   9c3ad:	48 8b 82 2f 02 00 00 	mov    0x22f(%rdx),%rax
   9c3b4:	48 89 8c 24 a8 12 00 	mov    %rcx,0x12a8(%rsp)
   9c3bb:	00 
   9c3bc:	48 8b 8a 47 02 00 00 	mov    0x247(%rdx),%rcx
   9c3c3:	0f 10 82 1f 02 00 00 	movups 0x21f(%rdx),%xmm0
   9c3ca:	0f 29 84 24 b0 12 00 	movaps %xmm0,0x12b0(%rsp)
   9c3d1:	00 
   9c3d2:	48 89 84 24 c0 12 00 	mov    %rax,0x12c0(%rsp)
   9c3d9:	00 
   9c3da:	48 8b 82 7f 02 00 00 	mov    0x27f(%rdx),%rax
   9c3e1:	0f 10 82 37 02 00 00 	movups 0x237(%rdx),%xmm0
   9c3e8:	0f 11 84 24 c8 12 00 	movups %xmm0,0x12c8(%rsp)
   9c3ef:	00 
   9c3f0:	0f 10 82 4f 02 00 00 	movups 0x24f(%rdx),%xmm0
   9c3f7:	48 89 8c 24 d8 12 00 	mov    %rcx,0x12d8(%rsp)
   9c3fe:	00 
   9c3ff:	0f 29 84 24 e0 12 00 	movaps %xmm0,0x12e0(%rsp)
   9c406:	00 
   9c407:	0f 10 82 5f 02 00 00 	movups 0x25f(%rdx),%xmm0
   9c40e:	0f 29 84 24 f0 12 00 	movaps %xmm0,0x12f0(%rsp)
   9c415:	00 
   9c416:	0f 10 82 6f 02 00 00 	movups 0x26f(%rdx),%xmm0
   9c41d:	0f 29 84 24 00 13 00 	movaps %xmm0,0x1300(%rsp)
   9c424:	00 
   9c425:	0f 10 82 87 02 00 00 	movups 0x287(%rdx),%xmm0
   9c42c:	48 89 84 24 10 13 00 	mov    %rax,0x1310(%rsp)
   9c433:	00 
   9c434:	0f 11 84 24 18 13 00 	movups %xmm0,0x1318(%rsp)
   9c43b:	00 
   9c43c:	0f 10 82 97 02 00 00 	movups 0x297(%rdx),%xmm0
   9c443:	0f 11 84 24 28 13 00 	movups %xmm0,0x1328(%rsp)
   9c44a:	00 
   9c44b:	0f 10 82 a7 02 00 00 	movups 0x2a7(%rdx),%xmm0
   9c452:	0f 11 84 24 38 13 00 	movups %xmm0,0x1338(%rsp)
   9c459:	00 
   9c45a:	0f 10 82 b7 02 00 00 	movups 0x2b7(%rdx),%xmm0
   9c461:	0f 11 84 24 48 13 00 	movups %xmm0,0x1348(%rsp)
   9c468:	00 
   9c469:	0f 10 82 c7 02 00 00 	movups 0x2c7(%rdx),%xmm0
   9c470:	0f 11 84 24 58 13 00 	movups %xmm0,0x1358(%rsp)
   9c477:	00 
   9c478:	0f 10 82 d7 02 00 00 	movups 0x2d7(%rdx),%xmm0
   9c47f:	0f 11 84 24 68 13 00 	movups %xmm0,0x1368(%rsp)
   9c486:	00 
   9c487:	0f 10 82 e7 02 00 00 	movups 0x2e7(%rdx),%xmm0
   9c48e:	0f 11 84 24 78 13 00 	movups %xmm0,0x1378(%rsp)
   9c495:	00 
   9c496:	0f 10 82 f7 02 00 00 	movups 0x2f7(%rdx),%xmm0
   9c49d:	0f 11 84 24 88 13 00 	movups %xmm0,0x1388(%rsp)
   9c4a4:	00 
   9c4a5:	0f 10 82 07 03 00 00 	movups 0x307(%rdx),%xmm0
   9c4ac:	48 8b 82 17 03 00 00 	mov    0x317(%rdx),%rax
   9c4b3:	48 8b 8a 3f 03 00 00 	mov    0x33f(%rdx),%rcx
   9c4ba:	0f 11 84 24 98 13 00 	movups %xmm0,0x1398(%rsp)
   9c4c1:	00 
   9c4c2:	48 89 84 24 a8 13 00 	mov    %rax,0x13a8(%rsp)
   9c4c9:	00 
   9c4ca:	48 8b 82 57 03 00 00 	mov    0x357(%rdx),%rax
   9c4d1:	0f 10 82 1f 03 00 00 	movups 0x31f(%rdx),%xmm0
   9c4d8:	0f 29 84 24 b0 13 00 	movaps %xmm0,0x13b0(%rsp)
   9c4df:	00 
   9c4e0:	0f 10 82 2f 03 00 00 	movups 0x32f(%rdx),%xmm0
   9c4e7:	0f 29 84 24 c0 13 00 	movaps %xmm0,0x13c0(%rsp)
   9c4ee:	00 
   9c4ef:	48 89 8c 24 d0 13 00 	mov    %rcx,0x13d0(%rsp)
   9c4f6:	00 
   9c4f7:	0f b6 8a 5f 03 00 00 	movzbl 0x35f(%rdx),%ecx
   9c4fe:	0f 10 82 47 03 00 00 	movups 0x347(%rdx),%xmm0
   9c505:	0f 11 84 24 d8 13 00 	movups %xmm0,0x13d8(%rsp)
   9c50c:	00 
   9c50d:	48 89 84 24 e8 13 00 	mov    %rax,0x13e8(%rsp)
   9c514:	00 
   9c515:	48 8b 82 90 03 00 00 	mov    0x390(%rdx),%rax
   9c51c:	48 89 8c 24 f0 13 00 	mov    %rcx,0x13f0(%rsp)
   9c523:	00 
   9c524:	0f b6 8a a8 03 00 00 	movzbl 0x3a8(%rdx),%ecx
   9c52b:	0f 10 82 60 03 00 00 	movups 0x360(%rdx),%xmm0
   9c532:	0f 11 84 24 f8 13 00 	movups %xmm0,0x13f8(%rsp)
   9c539:	00 
   9c53a:	0f 10 82 70 03 00 00 	movups 0x370(%rdx),%xmm0
   9c541:	0f 11 84 24 08 14 00 	movups %xmm0,0x1408(%rsp)
   9c548:	00 
   9c549:	0f 10 82 80 03 00 00 	movups 0x380(%rdx),%xmm0
   9c550:	0f 11 84 24 18 14 00 	movups %xmm0,0x1418(%rsp)
   9c557:	00 
   9c558:	48 89 84 24 28 14 00 	mov    %rax,0x1428(%rsp)
   9c55f:	00 
   9c560:	48 8b 82 b9 03 00 00 	mov    0x3b9(%rdx),%rax
   9c567:	0f 10 82 98 03 00 00 	movups 0x398(%rdx),%xmm0
   9c56e:	0f 29 84 24 30 14 00 	movaps %xmm0,0x1430(%rsp)
   9c575:	00 
   9c576:	48 89 8c 24 40 14 00 	mov    %rcx,0x1440(%rsp)
   9c57d:	00 
   9c57e:	48 8b 8a 01 04 00 00 	mov    0x401(%rdx),%rcx
   9c585:	0f 10 82 a9 03 00 00 	movups 0x3a9(%rdx),%xmm0
   9c58c:	0f 11 84 24 48 14 00 	movups %xmm0,0x1448(%rsp)
   9c593:	00 
   9c594:	0f 10 82 c1 03 00 00 	movups 0x3c1(%rdx),%xmm0
   9c59b:	48 89 84 24 58 14 00 	mov    %rax,0x1458(%rsp)
   9c5a2:	00 
   9c5a3:	0f 29 84 24 60 14 00 	movaps %xmm0,0x1460(%rsp)
   9c5aa:	00 
   9c5ab:	0f 10 82 d1 03 00 00 	movups 0x3d1(%rdx),%xmm0
   9c5b2:	0f 29 84 24 70 14 00 	movaps %xmm0,0x1470(%rsp)
   9c5b9:	00 
   9c5ba:	0f 10 82 e1 03 00 00 	movups 0x3e1(%rdx),%xmm0
   9c5c1:	0f 29 84 24 80 14 00 	movaps %xmm0,0x1480(%rsp)
   9c5c8:	00 
   9c5c9:	0f 10 82 f1 03 00 00 	movups 0x3f1(%rdx),%xmm0
   9c5d0:	0f 29 84 24 90 14 00 	movaps %xmm0,0x1490(%rsp)
   9c5d7:	00 
   9c5d8:	0f 10 82 09 04 00 00 	movups 0x409(%rdx),%xmm0
   9c5df:	48 89 8c 24 a0 14 00 	mov    %rcx,0x14a0(%rsp)
   9c5e6:	00 
   9c5e7:	0f 11 84 24 a8 14 00 	movups %xmm0,0x14a8(%rsp)
   9c5ee:	00 
   9c5ef:	0f 10 82 19 04 00 00 	movups 0x419(%rdx),%xmm0
   9c5f6:	0f b6 82 69 04 00 00 	movzbl 0x469(%rdx),%eax
   9c5fd:	0f b7 8a 6a 04 00 00 	movzwl 0x46a(%rdx),%ecx
   9c604:	c1 e1 08             	shl    $0x8,%ecx
   9c607:	48 09 c1             	or     %rax,%rcx
   9c60a:	48 8b 82 49 04 00 00 	mov    0x449(%rdx),%rax
   9c611:	0f 11 84 24 b8 14 00 	movups %xmm0,0x14b8(%rsp)
   9c618:	00 
   9c619:	0f 10 82 29 04 00 00 	movups 0x429(%rdx),%xmm0
   9c620:	0f 11 84 24 c8 14 00 	movups %xmm0,0x14c8(%rsp)
   9c627:	00 
   9c628:	0f 10 82 39 04 00 00 	movups 0x439(%rdx),%xmm0
   9c62f:	0f 11 84 24 d8 14 00 	movups %xmm0,0x14d8(%rsp)
   9c636:	00 
   9c637:	48 89 84 24 e8 14 00 	mov    %rax,0x14e8(%rsp)
   9c63e:	00 
   9c63f:	48 8b 82 61 04 00 00 	mov    0x461(%rdx),%rax
   9c646:	0f 10 82 51 04 00 00 	movups 0x451(%rdx),%xmm0
   9c64d:	0f 29 84 24 f0 14 00 	movaps %xmm0,0x14f0(%rsp)
   9c654:	00 
   9c655:	0f 10 82 6c 04 00 00 	movups 0x46c(%rdx),%xmm0
   9c65c:	48 89 84 24 00 15 00 	mov    %rax,0x1500(%rsp)
   9c663:	00 
   9c664:	48 89 8c 24 08 15 00 	mov    %rcx,0x1508(%rsp)
   9c66b:	00 
   9c66c:	0f 29 84 24 10 15 00 	movaps %xmm0,0x1510(%rsp)
   9c673:	00 
   9c674:	0f 10 82 7c 04 00 00 	movups 0x47c(%rdx),%xmm0
   9c67b:	0f b7 82 8c 04 00 00 	movzwl 0x48c(%rdx),%eax
   9c682:	0f 29 84 24 20 15 00 	movaps %xmm0,0x1520(%rsp)
   9c689:	00 
   9c68a:	0f 10 82 8e 04 00 00 	movups 0x48e(%rdx),%xmm0
   9c691:	48 89 84 24 30 15 00 	mov    %rax,0x1530(%rsp)
   9c698:	00 
   9c699:	0f 11 84 24 38 15 00 	movups %xmm0,0x1538(%rsp)
   9c6a0:	00 
   9c6a1:	0f 10 82 9e 04 00 00 	movups 0x49e(%rdx),%xmm0
   9c6a8:	0f b6 82 ae 04 00 00 	movzbl 0x4ae(%rdx),%eax
   9c6af:	0f 11 84 24 48 15 00 	movups %xmm0,0x1548(%rsp)
   9c6b6:	00 
   9c6b7:	0f 10 82 af 04 00 00 	movups 0x4af(%rdx),%xmm0
   9c6be:	88 84 24 58 15 00 00 	mov    %al,0x1558(%rsp)
   9c6c5:	0f 29 84 24 60 15 00 	movaps %xmm0,0x1560(%rsp)
   9c6cc:	00 
   9c6cd:	0f 10 82 bf 04 00 00 	movups 0x4bf(%rdx),%xmm0
   9c6d4:	0f 29 84 24 70 15 00 	movaps %xmm0,0x1570(%rsp)
   9c6db:	00 
   9c6dc:	f3 0f 6f 82 cf 04 00 	movdqu 0x4cf(%rdx),%xmm0
   9c6e3:	00 
   9c6e4:	66 0f 7f 84 24 80 15 	movdqa %xmm0,0x1580(%rsp)
   9c6eb:	00 00 
   9c6ed:	4c 8b 3a             	mov    (%rdx),%r15
   9c6f0:	48 8b 82 df 04 00 00 	mov    0x4df(%rdx),%rax
   9c6f7:	48 89 84 24 90 15 00 	mov    %rax,0x1590(%rsp)
   9c6fe:	00 
   9c6ff:	48 8b 44 24 70       	mov    0x70(%rsp),%rax
   9c704:	48 83 f8 04          	cmp    $0x4,%rax
   9c708:	0f 87 02 0b 00 00    	ja     9d210 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x3c10>
   9c70e:	b9 16 00 00 00       	mov    $0x16,%ecx
   9c713:	48 0f a3 c1          	bt     %rax,%rcx
   9c717:	0f 83 f3 0a 00 00    	jae    9d210 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x3c10>
   9c71d:	48 8b 8c 24 20 01 00 	mov    0x120(%rsp),%rcx
   9c724:	00 
   9c725:	48 85 c9             	test   %rcx,%rcx
   9c728:	0f 88 fb 26 00 00    	js     9ee29 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5829>
   9c72e:	89 cb                	mov    %ecx,%ebx
   9c730:	49 3b 5f 08          	cmp    0x8(%r15),%rbx
   9c734:	0f 8d ef 26 00 00    	jge    9ee29 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5829>
   9c73a:	49 8b 17             	mov    (%r15),%rdx
   9c73d:	4c 69 e3 60 01 00 00 	imul   $0x160,%rbx,%r12
   9c744:	4a 83 3c 22 06       	cmpq   $0x6,(%rdx,%r12,1)
   9c749:	0f 85 da 26 00 00    	jne    9ee29 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5829>
   9c74f:	48 c1 e9 20          	shr    $0x20,%rcx
   9c753:	4a 39 4c 22 19       	cmp    %rcx,0x19(%rdx,%r12,1)
   9c758:	0f 85 cb 26 00 00    	jne    9ee29 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5829>
   9c75e:	48 83 f8 01          	cmp    $0x1,%rax
   9c762:	0f 84 a7 09 00 00    	je     9d10f <_Wfl.fl_vm_exec$a5081fe92876eb37+0x3b0f>
   9c768:	48 83 f8 04          	cmp    $0x4,%rax
   9c76c:	0f 85 b5 09 00 00    	jne    9d127 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x3b27>
   9c772:	48 8d bc 24 70 10 00 	lea    0x1070(%rsp),%rdi
   9c779:	00 
   9c77a:	48 8d 74 24 70       	lea    0x70(%rsp),%rsi
   9c77f:	e8 4c c9 ff ff       	call   990d0 <_Wfl.fl_obj_index$6801bb8d311b3e64>
   9c784:	49 3b 47 08          	cmp    0x8(%r15),%rax
   9c788:	0f 83 6f 26 00 00    	jae    9edfd <_Wfl.fl_vm_exec$a5081fe92876eb37+0x57fd>
   9c78e:	49 8b 0f             	mov    (%r15),%rcx
   9c791:	48 69 c0 60 01 00 00 	imul   $0x160,%rax,%rax
   9c798:	48 8b 4c 01 11       	mov    0x11(%rcx,%rax,1),%rcx
   9c79d:	e9 03 0a 00 00       	jmp    9d1a5 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x3ba5>
   9c7a2:	e8 79 bd 0c 00       	call   168520 <__wolf_rt_strbuf_new>
   9c7a7:	48 89 c3             	mov    %rax,%rbx
   9c7aa:	ba 0d 00 00 00       	mov    $0xd,%edx
   9c7af:	48 89 c7             	mov    %rax,%rdi
   9c7b2:	48 8d 35 4a 82 f7 ff 	lea    -0x87db6(%rip),%rsi        # 14a03 <__abi_tag+0x14707>
   9c7b9:	31 c9                	xor    %ecx,%ecx
   9c7bb:	e8 10 be 0c 00       	call   1685d0 <__wolf_rt_strbuf_str>
   9c7c0:	4c 89 ef             	mov    %r13,%rdi
   9c7c3:	e8 38 9b 04 00       	call   e6300 <_Wfl.fl_type_name$425de879c2cc2a41>
   9c7c8:	48 89 df             	mov    %rbx,%rdi
   9c7cb:	48 89 c6             	mov    %rax,%rsi
   9c7ce:	31 c9                	xor    %ecx,%ecx
   9c7d0:	e8 fb bd 0c 00       	call   1685d0 <__wolf_rt_strbuf_str>
   9c7d5:	48 89 df             	mov    %rbx,%rdi
   9c7d8:	48 8d 9c 24 90 03 00 	lea    0x390(%rsp),%rbx
   9c7df:	00 
   9c7e0:	48 89 de             	mov    %rbx,%rsi
   9c7e3:	e8 e8 b9 0c 00       	call   1681d0 <__wolf_rt_strbuf_finish>
   9c7e8:	4c 8b 84 24 98 03 00 	mov    0x398(%rsp),%r8
   9c7ef:	00 
   9c7f0:	48 8b 0b             	mov    (%rbx),%rcx
   9c7f3:	ba 04 00 00 00       	mov    $0x4,%edx
   9c7f8:	4c 8b 74 24 10       	mov    0x10(%rsp),%r14
   9c7fd:	4c 89 f7             	mov    %r14,%rdi
   9c800:	48 8d 35 d9 28 f8 ff 	lea    -0x7d727(%rip),%rsi        # 1f0e0 <__abi_tag+0x1ede4>
   9c807:	4c 8d 8c 24 70 20 00 	lea    0x2070(%rsp),%r9
   9c80e:	00 
   9c80f:	e8 ec 1f 07 00       	call   10e800 <_Wfl.fl_raise_bytes.bytesview.4$e8b5fa06fc8adc2d>
   9c814:	48 8b 6c 24 18       	mov    0x18(%rsp),%rbp
   9c819:	e9 02 e8 ff ff       	jmp    9b020 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1a20>
   9c81e:	48 83 7c 24 20 2d    	cmpq   $0x2d,0x20(%rsp)
   9c824:	0f 85 d3 00 00 00    	jne    9c8fd <_Wfl.fl_vm_exec$a5081fe92876eb37+0x32fd>
   9c82a:	48 8d 48 fc          	lea    -0x4(%rax),%rcx
   9c82e:	48 83 f9 07          	cmp    $0x7,%rcx
   9c832:	77 53                	ja     9c887 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x3287>
   9c834:	48 8b 8c 24 c0 00 00 	mov    0xc0(%rsp),%rcx
   9c83b:	00 
   9c83c:	48 85 c9             	test   %rcx,%rcx
   9c83f:	0f 88 5b 24 00 00    	js     9eca0 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x56a0>
   9c845:	48 8b 54 24 10       	mov    0x10(%rsp),%rdx
   9c84a:	48 8b 12             	mov    (%rdx),%rdx
   9c84d:	89 ce                	mov    %ecx,%esi
   9c84f:	48 3b 72 08          	cmp    0x8(%rdx),%rsi
   9c853:	0f 8d 47 24 00 00    	jge    9eca0 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x56a0>
   9c859:	48 8b 12             	mov    (%rdx),%rdx
   9c85c:	48 69 f6 60 01 00 00 	imul   $0x160,%rsi,%rsi
   9c863:	48 39 04 32          	cmp    %rax,(%rdx,%rsi,1)
   9c867:	0f 85 33 24 00 00    	jne    9eca0 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x56a0>
   9c86d:	48 c1 e9 20          	shr    $0x20,%rcx
   9c871:	48 39 4c 32 19       	cmp    %rcx,0x19(%rdx,%rsi,1)
   9c876:	0f 85 24 24 00 00    	jne    9eca0 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x56a0>
   9c87c:	f6 44 32 09 02       	testb  $0x2,0x9(%rdx,%rsi,1)
   9c881:	0f 85 8c 12 00 00    	jne    9db13 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4513>
   9c887:	4c 8b 74 24 10       	mov    0x10(%rsp),%r14
   9c88c:	4c 89 f7             	mov    %r14,%rdi
   9c88f:	4c 89 ee             	mov    %r13,%rsi
   9c892:	48 89 ea             	mov    %rbp,%rdx
   9c895:	48 8d 9c 24 d0 00 00 	lea    0xd0(%rsp),%rbx
   9c89c:	00 
   9c89d:	48 89 d9             	mov    %rbx,%rcx
   9c8a0:	e8 6b 47 07 00       	call   111010 <_Wfl.fl_str_text$a33697b96957ba85>
   9c8a5:	4c 89 f7             	mov    %r14,%rdi
   9c8a8:	48 8d b4 24 b8 00 00 	lea    0xb8(%rsp),%rsi
   9c8af:	00 
   9c8b0:	48 89 da             	mov    %rbx,%rdx
   9c8b3:	48 8d 8c 24 38 05 00 	lea    0x538(%rsp),%rcx
   9c8ba:	00 
   9c8bb:	e8 30 6d ff ff       	call   935f0 <_Wfl.fl_map_set$540fcc92758450a0>
   9c8c0:	84 c0                	test   %al,%al
   9c8c2:	0f 85 62 1c 00 00    	jne    9e52a <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4f2a>
   9c8c8:	ba 0c 00 00 00       	mov    $0xc,%edx
   9c8cd:	41 b9 2d 00 00 00    	mov    $0x2d,%r9d
   9c8d3:	4c 8b 74 24 10       	mov    0x10(%rsp),%r14
   9c8d8:	4c 89 f7             	mov    %r14,%rdi
   9c8db:	48 8d 35 7d 7f f7 ff 	lea    -0x88083(%rip),%rsi        # 1485f <__abi_tag+0x14563>
   9c8e2:	48 8b 5c 24 08       	mov    0x8(%rsp),%rbx
   9c8e7:	48 89 d9             	mov    %rbx,%rcx
   9c8ea:	4c 8b 44 24 28       	mov    0x28(%rsp),%r8
   9c8ef:	6a 21                	push   $0x21
   9c8f1:	48 8d 05 41 81 f7 ff 	lea    -0x87ebf(%rip),%rax        # 14a39 <__abi_tag+0x1473d>
   9c8f8:	e9 09 0f 00 00       	jmp    9d806 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4206>
   9c8fd:	48 8b 7c 24 10       	mov    0x10(%rsp),%rdi
   9c902:	4c 89 ee             	mov    %r13,%rsi
   9c905:	48 89 ea             	mov    %rbp,%rdx
   9c908:	48 8d 8c 24 d0 00 00 	lea    0xd0(%rsp),%rcx
   9c90f:	00 
   9c910:	e8 fb 46 07 00       	call   111010 <_Wfl.fl_str_text$a33697b96957ba85>
   9c915:	48 83 bc 24 b8 00 00 	cmpq   $0x6,0xb8(%rsp)
   9c91c:	00 06 
   9c91e:	0f 84 a5 12 00 00    	je     9dbc9 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x45c9>
   9c924:	4c 8b 74 24 10       	mov    0x10(%rsp),%r14
   9c929:	49 8b 4e 20          	mov    0x20(%r14),%rcx
   9c92d:	49 8b 46 48          	mov    0x48(%r14),%rax
   9c931:	41 0f 10 06          	movups (%r14),%xmm0
   9c935:	0f 29 84 24 a0 15 00 	movaps %xmm0,0x15a0(%rsp)
   9c93c:	00 
   9c93d:	41 0f 10 46 10       	movups 0x10(%r14),%xmm0
   9c942:	0f 29 84 24 b0 15 00 	movaps %xmm0,0x15b0(%rsp)
   9c949:	00 
   9c94a:	48 89 8c 24 c0 15 00 	mov    %rcx,0x15c0(%rsp)
   9c951:	00 
   9c952:	41 0f b7 4e 50       	movzwl 0x50(%r14),%ecx
   9c957:	41 0f 10 46 28       	movups 0x28(%r14),%xmm0
   9c95c:	0f 11 84 24 c8 15 00 	movups %xmm0,0x15c8(%rsp)
   9c963:	00 
   9c964:	41 0f 10 46 38       	movups 0x38(%r14),%xmm0
   9c969:	0f 11 84 24 d8 15 00 	movups %xmm0,0x15d8(%rsp)
   9c970:	00 
   9c971:	48 89 84 24 e8 15 00 	mov    %rax,0x15e8(%rsp)
   9c978:	00 
   9c979:	49 8b 46 62          	mov    0x62(%r14),%rax
   9c97d:	48 89 8c 24 f0 15 00 	mov    %rcx,0x15f0(%rsp)
   9c984:	00 
   9c985:	49 8b 8e 8a 00 00 00 	mov    0x8a(%r14),%rcx
   9c98c:	41 0f 10 46 52       	movups 0x52(%r14),%xmm0
   9c991:	0f 11 84 24 f8 15 00 	movups %xmm0,0x15f8(%rsp)
   9c998:	00 
   9c999:	48 89 84 24 08 16 00 	mov    %rax,0x1608(%rsp)
   9c9a0:	00 
   9c9a1:	41 0f b7 86 a2 00 00 	movzwl 0xa2(%r14),%eax
   9c9a8:	00 
   9c9a9:	41 0f 10 46 6a       	movups 0x6a(%r14),%xmm0
   9c9ae:	0f 29 84 24 10 16 00 	movaps %xmm0,0x1610(%rsp)
   9c9b5:	00 
   9c9b6:	41 0f 10 46 7a       	movups 0x7a(%r14),%xmm0
   9c9bb:	0f 29 84 24 20 16 00 	movaps %xmm0,0x1620(%rsp)
   9c9c2:	00 
   9c9c3:	41 0f 10 86 92 00 00 	movups 0x92(%r14),%xmm0
   9c9ca:	00 
   9c9cb:	48 89 8c 24 30 16 00 	mov    %rcx,0x1630(%rsp)
   9c9d2:	00 
   9c9d3:	0f 11 84 24 38 16 00 	movups %xmm0,0x1638(%rsp)
   9c9da:	00 
   9c9db:	41 0f 10 86 a4 00 00 	movups 0xa4(%r14),%xmm0
   9c9e2:	00 
   9c9e3:	49 8b 8e c4 00 00 00 	mov    0xc4(%r14),%rcx
   9c9ea:	48 89 84 24 48 16 00 	mov    %rax,0x1648(%rsp)
   9c9f1:	00 
   9c9f2:	41 0f b6 86 5c 01 00 	movzbl 0x15c(%r14),%eax
   9c9f9:	00 
   9c9fa:	0f 29 84 24 50 16 00 	movaps %xmm0,0x1650(%rsp)
   9ca01:	00 
   9ca02:	41 0f 10 86 b4 00 00 	movups 0xb4(%r14),%xmm0
   9ca09:	00 
   9ca0a:	0f 29 84 24 60 16 00 	movaps %xmm0,0x1660(%rsp)
   9ca11:	00 
   9ca12:	41 0f 10 86 cc 00 00 	movups 0xcc(%r14),%xmm0
   9ca19:	00 
   9ca1a:	48 89 8c 24 70 16 00 	mov    %rcx,0x1670(%rsp)
   9ca21:	00 
   9ca22:	0f 11 84 24 78 16 00 	movups %xmm0,0x1678(%rsp)
   9ca29:	00 
   9ca2a:	41 0f 10 86 dc 00 00 	movups 0xdc(%r14),%xmm0
   9ca31:	00 
   9ca32:	0f 11 84 24 88 16 00 	movups %xmm0,0x1688(%rsp)
   9ca39:	00 
   9ca3a:	41 0f 10 86 ec 00 00 	movups 0xec(%r14),%xmm0
   9ca41:	00 
   9ca42:	0f 11 84 24 98 16 00 	movups %xmm0,0x1698(%rsp)
   9ca49:	00 
   9ca4a:	41 0f 10 86 fc 00 00 	movups 0xfc(%r14),%xmm0
   9ca51:	00 
   9ca52:	0f 11 84 24 a8 16 00 	movups %xmm0,0x16a8(%rsp)
   9ca59:	00 
   9ca5a:	41 0f 10 86 0c 01 00 	movups 0x10c(%r14),%xmm0
   9ca61:	00 
   9ca62:	0f 11 84 24 b8 16 00 	movups %xmm0,0x16b8(%rsp)
   9ca69:	00 
   9ca6a:	41 0f 10 86 1c 01 00 	movups 0x11c(%r14),%xmm0
   9ca71:	00 
   9ca72:	0f 11 84 24 c8 16 00 	movups %xmm0,0x16c8(%rsp)
   9ca79:	00 
   9ca7a:	41 0f 10 86 2c 01 00 	movups 0x12c(%r14),%xmm0
   9ca81:	00 
   9ca82:	0f 11 84 24 d8 16 00 	movups %xmm0,0x16d8(%rsp)
   9ca89:	00 
   9ca8a:	41 0f 10 86 3c 01 00 	movups 0x13c(%r14),%xmm0
   9ca91:	00 
   9ca92:	0f 11 84 24 e8 16 00 	movups %xmm0,0x16e8(%rsp)
   9ca99:	00 
   9ca9a:	41 0f 10 86 4c 01 00 	movups 0x14c(%r14),%xmm0
   9caa1:	00 
   9caa2:	0f 11 84 24 f8 16 00 	movups %xmm0,0x16f8(%rsp)
   9caa9:	00 
   9caaa:	41 0f 10 86 5d 01 00 	movups 0x15d(%r14),%xmm0
   9cab1:	00 
   9cab2:	88 84 24 08 17 00 00 	mov    %al,0x1708(%rsp)
   9cab9:	0f 29 84 24 10 17 00 	movaps %xmm0,0x1710(%rsp)
   9cac0:	00 
   9cac1:	41 0f 10 86 6d 01 00 	movups 0x16d(%r14),%xmm0
   9cac8:	00 
   9cac9:	41 0f b6 86 dd 01 00 	movzbl 0x1dd(%r14),%eax
   9cad0:	00 
   9cad1:	49 8b 8e ee 01 00 00 	mov    0x1ee(%r14),%rcx
   9cad8:	0f 29 84 24 20 17 00 	movaps %xmm0,0x1720(%rsp)
   9cadf:	00 
   9cae0:	41 0f 10 86 7d 01 00 	movups 0x17d(%r14),%xmm0
   9cae7:	00 
   9cae8:	0f 29 84 24 30 17 00 	movaps %xmm0,0x1730(%rsp)
   9caef:	00 
   9caf0:	41 0f 10 86 8d 01 00 	movups 0x18d(%r14),%xmm0
   9caf7:	00 
   9caf8:	0f 29 84 24 40 17 00 	movaps %xmm0,0x1740(%rsp)
   9caff:	00 
   9cb00:	41 0f 10 86 9d 01 00 	movups 0x19d(%r14),%xmm0
   9cb07:	00 
   9cb08:	0f 29 84 24 50 17 00 	movaps %xmm0,0x1750(%rsp)
   9cb0f:	00 
   9cb10:	41 0f 10 86 ad 01 00 	movups 0x1ad(%r14),%xmm0
   9cb17:	00 
   9cb18:	0f 29 84 24 60 17 00 	movaps %xmm0,0x1760(%rsp)
   9cb1f:	00 
   9cb20:	41 0f 10 86 bd 01 00 	movups 0x1bd(%r14),%xmm0
   9cb27:	00 
   9cb28:	0f 29 84 24 70 17 00 	movaps %xmm0,0x1770(%rsp)
   9cb2f:	00 
   9cb30:	41 0f 10 86 cd 01 00 	movups 0x1cd(%r14),%xmm0
   9cb37:	00 
   9cb38:	0f 29 84 24 80 17 00 	movaps %xmm0,0x1780(%rsp)
   9cb3f:	00 
   9cb40:	88 84 24 90 17 00 00 	mov    %al,0x1790(%rsp)
   9cb47:	49 8b 86 16 02 00 00 	mov    0x216(%r14),%rax
   9cb4e:	41 0f 10 86 de 01 00 	movups 0x1de(%r14),%xmm0
   9cb55:	00 
   9cb56:	0f 11 84 24 98 17 00 	movups %xmm0,0x1798(%rsp)
   9cb5d:	00 
   9cb5e:	48 89 8c 24 a8 17 00 	mov    %rcx,0x17a8(%rsp)
   9cb65:	00 
   9cb66:	41 0f b6 8e 1e 02 00 	movzbl 0x21e(%r14),%ecx
   9cb6d:	00 
   9cb6e:	41 0f 10 86 f6 01 00 	movups 0x1f6(%r14),%xmm0
   9cb75:	00 
   9cb76:	0f 29 84 24 b0 17 00 	movaps %xmm0,0x17b0(%rsp)
   9cb7d:	00 
   9cb7e:	41 0f 10 86 06 02 00 	movups 0x206(%r14),%xmm0
   9cb85:	00 
   9cb86:	0f 29 84 24 c0 17 00 	movaps %xmm0,0x17c0(%rsp)
   9cb8d:	00 
   9cb8e:	48 89 84 24 d0 17 00 	mov    %rax,0x17d0(%rsp)
   9cb95:	00 
   9cb96:	49 8b 86 2f 02 00 00 	mov    0x22f(%r14),%rax
   9cb9d:	48 89 8c 24 d8 17 00 	mov    %rcx,0x17d8(%rsp)
   9cba4:	00 
   9cba5:	49 8b 8e 47 02 00 00 	mov    0x247(%r14),%rcx
   9cbac:	41 0f 10 86 1f 02 00 	movups 0x21f(%r14),%xmm0
   9cbb3:	00 
   9cbb4:	0f 29 84 24 e0 17 00 	movaps %xmm0,0x17e0(%rsp)
   9cbbb:	00 
   9cbbc:	48 89 84 24 f0 17 00 	mov    %rax,0x17f0(%rsp)
   9cbc3:	00 
   9cbc4:	49 8b 86 7f 02 00 00 	mov    0x27f(%r14),%rax
   9cbcb:	41 0f 10 86 37 02 00 	movups 0x237(%r14),%xmm0
   9cbd2:	00 
   9cbd3:	0f 11 84 24 f8 17 00 	movups %xmm0,0x17f8(%rsp)
   9cbda:	00 
   9cbdb:	41 0f 10 86 4f 02 00 	movups 0x24f(%r14),%xmm0
   9cbe2:	00 
   9cbe3:	48 89 8c 24 08 18 00 	mov    %rcx,0x1808(%rsp)
   9cbea:	00 
   9cbeb:	0f 29 84 24 10 18 00 	movaps %xmm0,0x1810(%rsp)
   9cbf2:	00 
   9cbf3:	41 0f 10 86 5f 02 00 	movups 0x25f(%r14),%xmm0
   9cbfa:	00 
   9cbfb:	0f 29 84 24 20 18 00 	movaps %xmm0,0x1820(%rsp)
   9cc02:	00 
   9cc03:	41 0f 10 86 6f 02 00 	movups 0x26f(%r14),%xmm0
   9cc0a:	00 
   9cc0b:	0f 29 84 24 30 18 00 	movaps %xmm0,0x1830(%rsp)
   9cc12:	00 
   9cc13:	41 0f 10 86 87 02 00 	movups 0x287(%r14),%xmm0
   9cc1a:	00 
   9cc1b:	48 89 84 24 40 18 00 	mov    %rax,0x1840(%rsp)
   9cc22:	00 
   9cc23:	0f 11 84 24 48 18 00 	movups %xmm0,0x1848(%rsp)
   9cc2a:	00 
   9cc2b:	41 0f 10 86 97 02 00 	movups 0x297(%r14),%xmm0
   9cc32:	00 
   9cc33:	0f 11 84 24 58 18 00 	movups %xmm0,0x1858(%rsp)
   9cc3a:	00 
   9cc3b:	41 0f 10 86 a7 02 00 	movups 0x2a7(%r14),%xmm0
   9cc42:	00 
   9cc43:	0f 11 84 24 68 18 00 	movups %xmm0,0x1868(%rsp)
   9cc4a:	00 
   9cc4b:	41 0f 10 86 b7 02 00 	movups 0x2b7(%r14),%xmm0
   9cc52:	00 
   9cc53:	0f 11 84 24 78 18 00 	movups %xmm0,0x1878(%rsp)
   9cc5a:	00 
   9cc5b:	41 0f 10 86 c7 02 00 	movups 0x2c7(%r14),%xmm0
   9cc62:	00 
   9cc63:	0f 11 84 24 88 18 00 	movups %xmm0,0x1888(%rsp)
   9cc6a:	00 
   9cc6b:	41 0f 10 86 d7 02 00 	movups 0x2d7(%r14),%xmm0
   9cc72:	00 
   9cc73:	0f 11 84 24 98 18 00 	movups %xmm0,0x1898(%rsp)
   9cc7a:	00 
   9cc7b:	41 0f 10 86 e7 02 00 	movups 0x2e7(%r14),%xmm0
   9cc82:	00 
   9cc83:	0f 11 84 24 a8 18 00 	movups %xmm0,0x18a8(%rsp)
   9cc8a:	00 
   9cc8b:	41 0f 10 86 f7 02 00 	movups 0x2f7(%r14),%xmm0
   9cc92:	00 
   9cc93:	0f 11 84 24 b8 18 00 	movups %xmm0,0x18b8(%rsp)
   9cc9a:	00 
   9cc9b:	41 0f 10 86 07 03 00 	movups 0x307(%r14),%xmm0
   9cca2:	00 
   9cca3:	49 8b 86 17 03 00 00 	mov    0x317(%r14),%rax
   9ccaa:	49 8b 8e 3f 03 00 00 	mov    0x33f(%r14),%rcx
   9ccb1:	0f 11 84 24 c8 18 00 	movups %xmm0,0x18c8(%rsp)
   9ccb8:	00 
   9ccb9:	48 89 84 24 d8 18 00 	mov    %rax,0x18d8(%rsp)
   9ccc0:	00 
   9ccc1:	49 8b 86 57 03 00 00 	mov    0x357(%r14),%rax
   9ccc8:	41 0f 10 86 1f 03 00 	movups 0x31f(%r14),%xmm0
   9cccf:	00 
   9ccd0:	0f 29 84 24 e0 18 00 	movaps %xmm0,0x18e0(%rsp)
   9ccd7:	00 
   9ccd8:	41 0f 10 86 2f 03 00 	movups 0x32f(%r14),%xmm0
   9ccdf:	00 
   9cce0:	0f 29 84 24 f0 18 00 	movaps %xmm0,0x18f0(%rsp)
   9cce7:	00 
   9cce8:	48 89 8c 24 00 19 00 	mov    %rcx,0x1900(%rsp)
   9ccef:	00 
   9ccf0:	41 0f b6 8e 5f 03 00 	movzbl 0x35f(%r14),%ecx
   9ccf7:	00 
   9ccf8:	41 0f 10 86 47 03 00 	movups 0x347(%r14),%xmm0
   9ccff:	00 
   9cd00:	0f 11 84 24 08 19 00 	movups %xmm0,0x1908(%rsp)
   9cd07:	00 
   9cd08:	48 89 84 24 18 19 00 	mov    %rax,0x1918(%rsp)
   9cd0f:	00 
   9cd10:	49 8b 86 90 03 00 00 	mov    0x390(%r14),%rax
   9cd17:	48 89 8c 24 20 19 00 	mov    %rcx,0x1920(%rsp)
   9cd1e:	00 
   9cd1f:	41 0f b6 8e a8 03 00 	movzbl 0x3a8(%r14),%ecx
   9cd26:	00 
   9cd27:	41 0f 10 86 60 03 00 	movups 0x360(%r14),%xmm0
   9cd2e:	00 
   9cd2f:	0f 11 84 24 28 19 00 	movups %xmm0,0x1928(%rsp)
   9cd36:	00 
   9cd37:	41 0f 10 86 70 03 00 	movups 0x370(%r14),%xmm0
   9cd3e:	00 
   9cd3f:	0f 11 84 24 38 19 00 	movups %xmm0,0x1938(%rsp)
   9cd46:	00 
   9cd47:	41 0f 10 86 80 03 00 	movups 0x380(%r14),%xmm0
   9cd4e:	00 
   9cd4f:	0f 11 84 24 48 19 00 	movups %xmm0,0x1948(%rsp)
   9cd56:	00 
   9cd57:	48 89 84 24 58 19 00 	mov    %rax,0x1958(%rsp)
   9cd5e:	00 
   9cd5f:	49 8b 86 b9 03 00 00 	mov    0x3b9(%r14),%rax
   9cd66:	41 0f 10 86 98 03 00 	movups 0x398(%r14),%xmm0
   9cd6d:	00 
   9cd6e:	0f 29 84 24 60 19 00 	movaps %xmm0,0x1960(%rsp)
   9cd75:	00 
   9cd76:	48 89 8c 24 70 19 00 	mov    %rcx,0x1970(%rsp)
   9cd7d:	00 
   9cd7e:	49 8b 8e 01 04 00 00 	mov    0x401(%r14),%rcx
   9cd85:	41 0f 10 86 a9 03 00 	movups 0x3a9(%r14),%xmm0
   9cd8c:	00 
   9cd8d:	0f 11 84 24 78 19 00 	movups %xmm0,0x1978(%rsp)
   9cd94:	00 
   9cd95:	41 0f 10 86 c1 03 00 	movups 0x3c1(%r14),%xmm0
   9cd9c:	00 
   9cd9d:	48 89 84 24 88 19 00 	mov    %rax,0x1988(%rsp)
   9cda4:	00 
   9cda5:	0f 29 84 24 90 19 00 	movaps %xmm0,0x1990(%rsp)
   9cdac:	00 
   9cdad:	41 0f 10 86 d1 03 00 	movups 0x3d1(%r14),%xmm0
   9cdb4:	00 
   9cdb5:	0f 29 84 24 a0 19 00 	movaps %xmm0,0x19a0(%rsp)
   9cdbc:	00 
   9cdbd:	41 0f 10 86 e1 03 00 	movups 0x3e1(%r14),%xmm0
   9cdc4:	00 
   9cdc5:	0f 29 84 24 b0 19 00 	movaps %xmm0,0x19b0(%rsp)
   9cdcc:	00 
   9cdcd:	41 0f 10 86 f1 03 00 	movups 0x3f1(%r14),%xmm0
   9cdd4:	00 
   9cdd5:	0f 29 84 24 c0 19 00 	movaps %xmm0,0x19c0(%rsp)
   9cddc:	00 
   9cddd:	41 0f 10 86 09 04 00 	movups 0x409(%r14),%xmm0
   9cde4:	00 
   9cde5:	48 89 8c 24 d0 19 00 	mov    %rcx,0x19d0(%rsp)
   9cdec:	00 
   9cded:	0f 11 84 24 d8 19 00 	movups %xmm0,0x19d8(%rsp)
   9cdf4:	00 
   9cdf5:	41 0f 10 86 19 04 00 	movups 0x419(%r14),%xmm0
   9cdfc:	00 
   9cdfd:	41 0f b6 86 69 04 00 	movzbl 0x469(%r14),%eax
   9ce04:	00 
   9ce05:	41 0f b7 8e 6a 04 00 	movzwl 0x46a(%r14),%ecx
   9ce0c:	00 
   9ce0d:	c1 e1 08             	shl    $0x8,%ecx
   9ce10:	48 09 c1             	or     %rax,%rcx
   9ce13:	49 8b 86 49 04 00 00 	mov    0x449(%r14),%rax
   9ce1a:	0f 11 84 24 e8 19 00 	movups %xmm0,0x19e8(%rsp)
   9ce21:	00 
   9ce22:	41 0f 10 86 29 04 00 	movups 0x429(%r14),%xmm0
   9ce29:	00 
   9ce2a:	0f 11 84 24 f8 19 00 	movups %xmm0,0x19f8(%rsp)
   9ce31:	00 
   9ce32:	41 0f 10 86 39 04 00 	movups 0x439(%r14),%xmm0
   9ce39:	00 
   9ce3a:	0f 11 84 24 08 1a 00 	movups %xmm0,0x1a08(%rsp)
   9ce41:	00 
   9ce42:	48 89 84 24 18 1a 00 	mov    %rax,0x1a18(%rsp)
   9ce49:	00 
   9ce4a:	49 8b 86 61 04 00 00 	mov    0x461(%r14),%rax
   9ce51:	41 0f 10 86 51 04 00 	movups 0x451(%r14),%xmm0
   9ce58:	00 
   9ce59:	0f 29 84 24 20 1a 00 	movaps %xmm0,0x1a20(%rsp)
   9ce60:	00 
   9ce61:	48 89 84 24 30 1a 00 	mov    %rax,0x1a30(%rsp)
   9ce68:	00 
   9ce69:	0f 28 44 24 50       	movaps 0x50(%rsp),%xmm0
   9ce6e:	0f 29 84 24 30 01 00 	movaps %xmm0,0x130(%rsp)
   9ce75:	00 
   9ce76:	41 0f 10 86 6c 04 00 	movups 0x46c(%r14),%xmm0
   9ce7d:	00 
   9ce7e:	48 89 8c 24 38 1a 00 	mov    %rcx,0x1a38(%rsp)
   9ce85:	00 
   9ce86:	0f 29 84 24 40 1a 00 	movaps %xmm0,0x1a40(%rsp)
   9ce8d:	00 
   9ce8e:	41 0f 10 86 7c 04 00 	movups 0x47c(%r14),%xmm0
   9ce95:	00 
   9ce96:	41 0f b7 86 8c 04 00 	movzwl 0x48c(%r14),%eax
   9ce9d:	00 
   9ce9e:	0f 29 84 24 50 1a 00 	movaps %xmm0,0x1a50(%rsp)
   9cea5:	00 
   9cea6:	41 0f 10 86 8e 04 00 	movups 0x48e(%r14),%xmm0
   9cead:	00 
   9ceae:	48 89 84 24 60 1a 00 	mov    %rax,0x1a60(%rsp)
   9ceb5:	00 
   9ceb6:	0f 11 84 24 68 1a 00 	movups %xmm0,0x1a68(%rsp)
   9cebd:	00 
   9cebe:	41 0f 10 86 9e 04 00 	movups 0x49e(%r14),%xmm0
   9cec5:	00 
   9cec6:	41 0f b6 86 ae 04 00 	movzbl 0x4ae(%r14),%eax
   9cecd:	00 
   9cece:	0f 11 84 24 78 1a 00 	movups %xmm0,0x1a78(%rsp)
   9ced5:	00 
   9ced6:	41 0f 10 86 af 04 00 	movups 0x4af(%r14),%xmm0
   9cedd:	00 
   9cede:	88 84 24 88 1a 00 00 	mov    %al,0x1a88(%rsp)
   9cee5:	0f 29 84 24 90 1a 00 	movaps %xmm0,0x1a90(%rsp)
   9ceec:	00 
   9ceed:	41 0f 10 86 bf 04 00 	movups 0x4bf(%r14),%xmm0
   9cef4:	00 
   9cef5:	0f 29 84 24 a0 1a 00 	movaps %xmm0,0x1aa0(%rsp)
   9cefc:	00 
   9cefd:	f3 41 0f 6f 86 cf 04 	movdqu 0x4cf(%r14),%xmm0
   9cf04:	00 00 
   9cf06:	66 0f 7f 84 24 b0 1a 	movdqa %xmm0,0x1ab0(%rsp)
   9cf0d:	00 00 
   9cf0f:	48 8b 44 24 60       	mov    0x60(%rsp),%rax
   9cf14:	48 89 84 24 40 01 00 	mov    %rax,0x140(%rsp)
   9cf1b:	00 
   9cf1c:	49 8b 86 df 04 00 00 	mov    0x4df(%r14),%rax
   9cf23:	48 89 84 24 c0 1a 00 	mov    %rax,0x1ac0(%rsp)
   9cf2a:	00 
   9cf2b:	48 8d bc 24 a0 15 00 	lea    0x15a0(%rsp),%rdi
   9cf32:	00 
   9cf33:	48 8d b4 24 b8 00 00 	lea    0xb8(%rsp),%rsi
   9cf3a:	00 
   9cf3b:	4c 89 ea             	mov    %r13,%rdx
   9cf3e:	48 89 e9             	mov    %rbp,%rcx
   9cf41:	e8 3a 9b 04 00       	call   e6a80 <_Wfl.fl_missing_field$f6f60a2fd600a704>
   9cf46:	ba 03 00 00 00       	mov    $0x3,%edx
   9cf4b:	4c 89 f7             	mov    %r14,%rdi
   9cf4e:	48 8d 35 2e 7a f7 ff 	lea    -0x885d2(%rip),%rsi        # 14983 <__abi_tag+0x14687>
   9cf55:	48 89 c1             	mov    %rax,%rcx
   9cf58:	4c 8d 84 24 28 20 00 	lea    0x2028(%rsp),%r8
   9cf5f:	00 
   9cf60:	e8 db 23 03 00       	call   cf340 <_Wfl.fl_raise_bytes$05a094d3779625bf>
   9cf65:	48 8b 6c 24 18       	mov    0x18(%rsp),%rbp
   9cf6a:	e9 b1 e0 ff ff       	jmp    9b020 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1a20>
   9cf6f:	49 83 fd 04          	cmp    $0x4,%r13
   9cf73:	0f 87 a1 08 00 00    	ja     9d81a <_Wfl.fl_vm_exec$a5081fe92876eb37+0x421a>
   9cf79:	ba 16 00 00 00       	mov    $0x16,%edx
   9cf7e:	4c 0f a3 ea          	bt     %r13,%rdx
   9cf82:	0f 83 92 08 00 00    	jae    9d81a <_Wfl.fl_vm_exec$a5081fe92876eb37+0x421a>
   9cf88:	48 8d 51 f4          	lea    -0xc(%rcx),%rdx
   9cf8c:	48 83 fa f8          	cmp    $0xfffffffffffffff8,%rdx
   9cf90:	0f 82 54 1e 00 00    	jb     9edea <_Wfl.fl_vm_exec$a5081fe92876eb37+0x57ea>
   9cf96:	48 85 c0             	test   %rax,%rax
   9cf99:	0f 88 4b 1e 00 00    	js     9edea <_Wfl.fl_vm_exec$a5081fe92876eb37+0x57ea>
   9cf9f:	89 c3                	mov    %eax,%ebx
   9cfa1:	48 3b 5d 08          	cmp    0x8(%rbp),%rbx
   9cfa5:	0f 8d 3f 1e 00 00    	jge    9edea <_Wfl.fl_vm_exec$a5081fe92876eb37+0x57ea>
   9cfab:	48 8b 55 00          	mov    0x0(%rbp),%rdx
   9cfaf:	4c 69 e3 60 01 00 00 	imul   $0x160,%rbx,%r12
   9cfb6:	4a 39 0c 22          	cmp    %rcx,(%rdx,%r12,1)
   9cfba:	0f 85 2a 1e 00 00    	jne    9edea <_Wfl.fl_vm_exec$a5081fe92876eb37+0x57ea>
   9cfc0:	48 c1 e8 20          	shr    $0x20,%rax
   9cfc4:	4a 39 44 22 19       	cmp    %rax,0x19(%rdx,%r12,1)
   9cfc9:	0f 85 1b 1e 00 00    	jne    9edea <_Wfl.fl_vm_exec$a5081fe92876eb37+0x57ea>
   9cfcf:	49 83 fd 01          	cmp    $0x1,%r13
   9cfd3:	0f 84 cc 06 00 00    	je     9d6a5 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x40a5>
   9cfd9:	49 83 fd 04          	cmp    $0x4,%r13
   9cfdd:	0f 85 dd 06 00 00    	jne    9d6c0 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x40c0>
   9cfe3:	48 8d bc 24 10 06 00 	lea    0x610(%rsp),%rdi
   9cfea:	00 
   9cfeb:	48 8d b4 24 88 00 00 	lea    0x88(%rsp),%rsi
   9cff2:	00 
   9cff3:	e8 d8 c0 ff ff       	call   990d0 <_Wfl.fl_obj_index$6801bb8d311b3e64>
   9cff8:	48 3b 45 08          	cmp    0x8(%rbp),%rax
   9cffc:	0f 83 d5 1c 00 00    	jae    9ecd7 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x56d7>
   9d002:	48 8b 4d 00          	mov    0x0(%rbp),%rcx
   9d006:	48 69 c0 60 01 00 00 	imul   $0x160,%rax,%rax
   9d00d:	48 8b 4c 01 11       	mov    0x11(%rcx,%rax,1),%rcx
   9d012:	e9 29 07 00 00       	jmp    9d740 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4140>
   9d017:	ba 04 00 00 00       	mov    $0x4,%edx
   9d01c:	41 b8 19 00 00 00    	mov    $0x19,%r8d
   9d022:	4c 8b 74 24 10       	mov    0x10(%rsp),%r14
   9d027:	4c 89 f7             	mov    %r14,%rdi
   9d02a:	48 8d 35 af 20 f8 ff 	lea    -0x7df51(%rip),%rsi        # 1f0e0 <__abi_tag+0x1ede4>
   9d031:	48 8d 0d 65 79 f7 ff 	lea    -0x8869b(%rip),%rcx        # 1499d <__abi_tag+0x146a1>
   9d038:	4c 8d 8c 24 d0 20 00 	lea    0x20d0(%rsp),%r9
   9d03f:	00 
   9d040:	e9 ca f7 ff ff       	jmp    9c80f <_Wfl.fl_vm_exec$a5081fe92876eb37+0x320f>
   9d045:	49 83 fd 01          	cmp    $0x1,%r13
   9d049:	0f 85 b3 08 00 00    	jne    9d902 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4302>
   9d04f:	b8 6b ca eb 85       	mov    $0x85ebca6b,%eax
   9d054:	48 8d 88 4e af 4b 18 	lea    0x184baf4e(%rax),%rcx
   9d05b:	48 83 7c 24 38 00    	cmpq   $0x0,0x38(%rsp)
   9d061:	48 0f 44 c8          	cmove  %rax,%rcx
   9d065:	e9 18 09 00 00       	jmp    9d982 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4382>
   9d06a:	e8 b1 b4 0c 00       	call   168520 <__wolf_rt_strbuf_new>
   9d06f:	48 89 c3             	mov    %rax,%rbx
   9d072:	ba 06 00 00 00       	mov    $0x6,%edx
   9d077:	48 89 c7             	mov    %rax,%rdi
   9d07a:	48 8d 35 3a 79 f7 ff 	lea    -0x886c6(%rip),%rsi        # 149bb <__abi_tag+0x146bf>
   9d081:	31 c9                	xor    %ecx,%ecx
   9d083:	e8 48 b5 0c 00       	call   1685d0 <__wolf_rt_strbuf_str>
   9d088:	48 89 df             	mov    %rbx,%rdi
   9d08b:	4c 89 ee             	mov    %r13,%rsi
   9d08e:	31 d2                	xor    %edx,%edx
   9d090:	e8 4b b3 0c 00       	call   1683e0 <__wolf_rt_strbuf_i64>
   9d095:	ba 13 00 00 00       	mov    $0x13,%edx
   9d09a:	48 89 df             	mov    %rbx,%rdi
   9d09d:	48 8d 35 1d 79 f7 ff 	lea    -0x886e3(%rip),%rsi        # 149c1 <__abi_tag+0x146c5>
   9d0a4:	31 c9                	xor    %ecx,%ecx
   9d0a6:	e8 25 b5 0c 00       	call   1685d0 <__wolf_rt_strbuf_str>
   9d0ab:	48 89 df             	mov    %rbx,%rdi
   9d0ae:	4c 89 f6             	mov    %r14,%rsi
   9d0b1:	31 d2                	xor    %edx,%edx
   9d0b3:	e8 28 b3 0c 00       	call   1683e0 <__wolf_rt_strbuf_i64>
   9d0b8:	ba 01 00 00 00       	mov    $0x1,%edx
   9d0bd:	48 89 df             	mov    %rbx,%rdi
   9d0c0:	48 8d 35 a9 74 f7 ff 	lea    -0x88b57(%rip),%rsi        # 14570 <__abi_tag+0x14274>
   9d0c7:	31 c9                	xor    %ecx,%ecx
   9d0c9:	e8 02 b5 0c 00       	call   1685d0 <__wolf_rt_strbuf_str>
   9d0ce:	48 89 df             	mov    %rbx,%rdi
   9d0d1:	48 8d b4 24 b0 01 00 	lea    0x1b0(%rsp),%rsi
   9d0d8:	00 
   9d0d9:	e8 f2 b0 0c 00       	call   1681d0 <__wolf_rt_strbuf_finish>
   9d0de:	48 8b 8c 24 b0 01 00 	mov    0x1b0(%rsp),%rcx
   9d0e5:	00 
   9d0e6:	4c 8b 84 24 b8 01 00 	mov    0x1b8(%rsp),%r8
   9d0ed:	00 
   9d0ee:	ba 05 00 00 00       	mov    $0x5,%edx
   9d0f3:	4c 8b 74 24 10       	mov    0x10(%rsp),%r14
   9d0f8:	4c 89 f7             	mov    %r14,%rdi
   9d0fb:	48 8d 35 b4 78 f7 ff 	lea    -0x8874c(%rip),%rsi        # 149b6 <__abi_tag+0x146ba>
   9d102:	4c 8d 8c 24 b8 20 00 	lea    0x20b8(%rsp),%r9
   9d109:	00 
   9d10a:	e9 00 f7 ff ff       	jmp    9c80f <_Wfl.fl_vm_exec$a5081fe92876eb37+0x320f>
   9d10f:	b8 6b ca eb 85       	mov    $0x85ebca6b,%eax
   9d114:	48 8d 88 4e af 4b 18 	lea    0x184baf4e(%rax),%rcx
   9d11b:	48 83 7c 24 78 00    	cmpq   $0x0,0x78(%rsp)
   9d121:	48 0f 44 c8          	cmove  %rax,%rcx
   9d125:	eb 7e                	jmp    9d1a5 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x3ba5>
   9d127:	48 8b 44 24 78       	mov    0x78(%rsp),%rax
   9d12c:	0f b6 c8             	movzbl %al,%ecx
   9d12f:	81 f1 c5 9d 1c 81    	xor    $0x811c9dc5,%ecx
   9d135:	69 c9 93 01 00 01    	imul   $0x1000193,%ecx,%ecx
   9d13b:	0f b6 d4             	movzbl %ah,%edx
   9d13e:	31 ca                	xor    %ecx,%edx
   9d140:	69 ca 93 01 00 01    	imul   $0x1000193,%edx,%ecx
   9d146:	89 c2                	mov    %eax,%edx
   9d148:	c1 ea 10             	shr    $0x10,%edx
   9d14b:	0f b6 d2             	movzbl %dl,%edx
   9d14e:	31 ca                	xor    %ecx,%edx
   9d150:	69 ca 93 01 00 01    	imul   $0x1000193,%edx,%ecx
   9d156:	89 c2                	mov    %eax,%edx
   9d158:	c1 ea 18             	shr    $0x18,%edx
   9d15b:	31 ca                	xor    %ecx,%edx
   9d15d:	69 ca 93 01 00 01    	imul   $0x1000193,%edx,%ecx
   9d163:	48 89 c2             	mov    %rax,%rdx
   9d166:	48 c1 ea 20          	shr    $0x20,%rdx
   9d16a:	0f b6 d2             	movzbl %dl,%edx
   9d16d:	31 ca                	xor    %ecx,%edx
   9d16f:	69 ca 93 01 00 01    	imul   $0x1000193,%edx,%ecx
   9d175:	48 89 c2             	mov    %rax,%rdx
   9d178:	48 c1 ea 28          	shr    $0x28,%rdx
   9d17c:	0f b6 d2             	movzbl %dl,%edx
   9d17f:	31 ca                	xor    %ecx,%edx
   9d181:	69 ca 93 01 00 01    	imul   $0x1000193,%edx,%ecx
   9d187:	48 89 c2             	mov    %rax,%rdx
   9d18a:	48 c1 ea 30          	shr    $0x30,%rdx
   9d18e:	0f b6 d2             	movzbl %dl,%edx
   9d191:	31 ca                	xor    %ecx,%edx
   9d193:	69 ca 93 01 00 01    	imul   $0x1000193,%edx,%ecx
   9d199:	48 c1 e8 38          	shr    $0x38,%rax
   9d19d:	31 c8                	xor    %ecx,%eax
   9d19f:	69 c8 93 01 00 01    	imul   $0x1000193,%eax,%ecx
   9d1a5:	48 8d bc 24 70 10 00 	lea    0x1070(%rsp),%rdi
   9d1ac:	00 
   9d1ad:	48 89 de             	mov    %rbx,%rsi
   9d1b0:	48 8d 54 24 70       	lea    0x70(%rsp),%rdx
   9d1b5:	e8 16 f6 07 00       	call   11c7d0 <_Wfl.fl_map_probe$234ad4c981fd6647>
   9d1ba:	84 d2                	test   %dl,%dl
   9d1bc:	74 52                	je     9d210 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x3c10>
   9d1be:	49 3b 5f 08          	cmp    0x8(%r15),%rbx
   9d1c2:	0f 83 35 1c 00 00    	jae    9edfd <_Wfl.fl_vm_exec$a5081fe92876eb37+0x57fd>
   9d1c8:	49 8b 0f             	mov    (%r15),%rcx
   9d1cb:	4a 8b 54 21 39       	mov    0x39(%rcx,%r12,1),%rdx
   9d1d0:	48 3b 42 08          	cmp    0x8(%rdx),%rax
   9d1d4:	0f 83 23 1c 00 00    	jae    9edfd <_Wfl.fl_vm_exec$a5081fe92876eb37+0x57fd>
   9d1da:	48 8b 12             	mov    (%rdx),%rdx
   9d1dd:	48 8b 04 c2          	mov    (%rdx,%rax,8),%rax
   9d1e1:	48 ff c8             	dec    %rax
   9d1e4:	0f 80 4b 19 00 00    	jo     9eb35 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5535>
   9d1ea:	4a 8b 4c 21 31       	mov    0x31(%rcx,%r12,1),%rcx
   9d1ef:	48 3b 41 08          	cmp    0x8(%rcx),%rax
   9d1f3:	0f 83 04 1c 00 00    	jae    9edfd <_Wfl.fl_vm_exec$a5081fe92876eb37+0x57fd>
   9d1f9:	48 8b 11             	mov    (%rcx),%rdx
   9d1fc:	48 c1 e0 06          	shl    $0x6,%rax
   9d200:	48 8d 0c 02          	lea    (%rdx,%rax,1),%rcx
   9d204:	48 83 c1 18          	add    $0x18,%rcx
   9d208:	66 0f 6f 44 02 20    	movdqa 0x20(%rdx,%rax,1),%xmm0
   9d20e:	eb 36                	jmp    9d246 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x3c46>
   9d210:	ba 03 00 00 00       	mov    $0x3,%edx
   9d215:	41 b8 0b 00 00 00    	mov    $0xb,%r8d
   9d21b:	48 8b 7c 24 10       	mov    0x10(%rsp),%rdi
   9d220:	48 8d 35 5c 77 f7 ff 	lea    -0x888a4(%rip),%rsi        # 14983 <__abi_tag+0x14687>
   9d227:	48 8d 0d ca 77 f7 ff 	lea    -0x88836(%rip),%rcx        # 149f8 <__abi_tag+0x146fc>
   9d22e:	4c 8d 8c 24 a0 20 00 	lea    0x20a0(%rsp),%r9
   9d235:	00 
   9d236:	e8 c5 15 07 00       	call   10e800 <_Wfl.fl_raise_bytes.bytesview.4$e8b5fa06fc8adc2d>
   9d23b:	f3 0f 6f 44 24 58    	movdqu 0x58(%rsp),%xmm0
   9d241:	48 8d 4c 24 50       	lea    0x50(%rsp),%rcx
   9d246:	48 8b 01             	mov    (%rcx),%rax
   9d249:	48 89 84 24 50 05 00 	mov    %rax,0x550(%rsp)
   9d250:	00 
   9d251:	f3 0f 7f 84 24 58 05 	movdqu %xmm0,0x558(%rsp)
   9d258:	00 00 
   9d25a:	48 8b 44 24 10       	mov    0x10(%rsp),%rax
   9d25f:	48 05 5c 01 00 00    	add    $0x15c,%rax
   9d265:	80 38 00             	cmpb   $0x0,(%rax)
   9d268:	0f 85 bc 12 00 00    	jne    9e52a <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4f2a>
   9d26e:	4c 8b 74 24 10       	mov    0x10(%rsp),%r14
   9d273:	4c 89 f7             	mov    %r14,%rdi
   9d276:	48 8d b4 24 50 05 00 	lea    0x550(%rsp),%rsi
   9d27d:	00 
   9d27e:	e9 49 d2 ff ff       	jmp    9a4cc <_Wfl.fl_vm_exec$a5081fe92876eb37+0xecc>
   9d283:	49 ff ca             	dec    %r10
   9d286:	0f 80 85 19 00 00    	jo     9ec11 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5611>
   9d28c:	4c 3b 56 08          	cmp    0x8(%rsi),%r10
   9d290:	0f 83 eb 1b 00 00    	jae    9ee81 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5881>
   9d296:	48 8b 06             	mov    (%rsi),%rax
   9d299:	4b 8d 14 52          	lea    (%r10,%r10,2),%rdx
   9d29d:	48 8b 0c d0          	mov    (%rax,%rdx,8),%rcx
   9d2a1:	48 8d 71 fb          	lea    -0x5(%rcx),%rsi
   9d2a5:	48 83 fe 01          	cmp    $0x1,%rsi
   9d2a9:	0f 87 1d 08 00 00    	ja     9dacc <_Wfl.fl_vm_exec$a5081fe92876eb37+0x44cc>
   9d2af:	48 8b 44 d0 08       	mov    0x8(%rax,%rdx,8),%rax
   9d2b4:	48 85 c0             	test   %rax,%rax
   9d2b7:	0f 88 88 1a 00 00    	js     9ed45 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5745>
   9d2bd:	89 c6                	mov    %eax,%esi
   9d2bf:	49 3b 70 08          	cmp    0x8(%r8),%rsi
   9d2c3:	0f 8d 7c 1a 00 00    	jge    9ed45 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5745>
   9d2c9:	49 8b 10             	mov    (%r8),%rdx
   9d2cc:	48 69 f6 60 01 00 00 	imul   $0x160,%rsi,%rsi
   9d2d3:	48 39 0c 32          	cmp    %rcx,(%rdx,%rsi,1)
   9d2d7:	0f 85 68 1a 00 00    	jne    9ed45 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5745>
   9d2dd:	48 c1 e8 20          	shr    $0x20,%rax
   9d2e1:	48 39 44 32 19       	cmp    %rax,0x19(%rdx,%rsi,1)
   9d2e6:	0f 85 59 1a 00 00    	jne    9ed45 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5745>
   9d2ec:	48 8b 5c 32 59       	mov    0x59(%rdx,%rsi,1),%rbx
   9d2f1:	e9 d8 07 00 00       	jmp    9dace <_Wfl.fl_vm_exec$a5081fe92876eb37+0x44ce>
   9d2f6:	4c 8b 74 24 10       	mov    0x10(%rsp),%r14
   9d2fb:	4c 89 f7             	mov    %r14,%rdi
   9d2fe:	4c 89 ee             	mov    %r13,%rsi
   9d301:	48 8b 5c 24 08       	mov    0x8(%rsp),%rbx
   9d306:	48 89 da             	mov    %rbx,%rdx
   9d309:	e8 b2 40 02 00       	call   c13c0 <_Wfl.fl_read16$7e28ba906e98be2a>
   9d30e:	ba 06 00 00 00       	mov    $0x6,%edx
   9d313:	41 b8 0e 00 00 00    	mov    $0xe,%r8d
   9d319:	4c 89 f7             	mov    %r14,%rdi
   9d31c:	48 8d 35 37 77 f7 ff 	lea    -0x888c9(%rip),%rsi        # 14a5a <__abi_tag+0x1475e>
   9d323:	48 8d 0d 36 77 f7 ff 	lea    -0x888ca(%rip),%rcx        # 14a60 <__abi_tag+0x14764>
   9d32a:	4c 8d 8c 24 10 20 00 	lea    0x2010(%rsp),%r9
   9d331:	00 
   9d332:	e8 c9 14 07 00       	call   10e800 <_Wfl.fl_raise_bytes.bytesview.4$e8b5fa06fc8adc2d>
   9d337:	48 8b 6c 24 18       	mov    0x18(%rsp),%rbp
   9d33c:	e9 e4 dc ff ff       	jmp    9b025 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1a25>
   9d341:	4c 8b 74 24 10       	mov    0x10(%rsp),%r14
   9d346:	4c 89 f7             	mov    %r14,%rdi
   9d349:	4c 89 ee             	mov    %r13,%rsi
   9d34c:	4c 8b 7c 24 08       	mov    0x8(%rsp),%r15
   9d351:	4c 89 fa             	mov    %r15,%rdx
   9d354:	e8 67 40 02 00       	call   c13c0 <_Wfl.fl_read16$7e28ba906e98be2a>
   9d359:	48 89 c3             	mov    %rax,%rbx
   9d35c:	4c 89 f7             	mov    %r14,%rdi
   9d35f:	4c 89 ee             	mov    %r13,%rsi
   9d362:	4c 89 fa             	mov    %r15,%rdx
   9d365:	e8 a6 3f 02 00       	call   c1310 <_Wfl.fl_read8$a2ddcd4dfd790b8d>
   9d36a:	49 8b 0e             	mov    (%r14),%rcx
   9d36d:	4c 3b 79 08          	cmp    0x8(%rcx),%r15
   9d371:	0f 83 dc 18 00 00    	jae    9ec53 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5653>
   9d377:	48 8b 09             	mov    (%rcx),%rcx
   9d37a:	48 69 54 24 08 60 01 	imul   $0x160,0x8(%rsp),%rdx
   9d381:	00 00 
   9d383:	48 8b 4c 11 29       	mov    0x29(%rcx,%rdx,1),%rcx
   9d388:	48 3b 59 08          	cmp    0x8(%rcx),%rbx
   9d38c:	0f 83 c1 18 00 00    	jae    9ec53 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5653>
   9d392:	48 8b 09             	mov    (%rcx),%rcx
   9d395:	48 8d 14 5b          	lea    (%rbx,%rbx,2),%rdx
   9d399:	48 8b 74 d1 08       	mov    0x8(%rcx,%rdx,8),%rsi
   9d39e:	31 d2                	xor    %edx,%edx
   9d3a0:	48 85 c0             	test   %rax,%rax
   9d3a3:	0f 95 c2             	setne  %dl
   9d3a6:	48 8b 5c 24 10       	mov    0x10(%rsp),%rbx
   9d3ab:	48 89 df             	mov    %rbx,%rdi
   9d3ae:	48 8d 8c 24 98 05 00 	lea    0x598(%rsp),%rcx
   9d3b5:	00 
   9d3b6:	e8 d5 5e 06 00       	call   103290 <_Wfl.fl_import$c2bff798315c274a>
   9d3bb:	80 bb 5c 01 00 00 00 	cmpb   $0x0,0x15c(%rbx)
   9d3c2:	0f 85 62 11 00 00    	jne    9e52a <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4f2a>
   9d3c8:	4c 8b 74 24 10       	mov    0x10(%rsp),%r14
   9d3cd:	4c 89 f7             	mov    %r14,%rdi
   9d3d0:	48 8d b4 24 98 05 00 	lea    0x598(%rsp),%rsi
   9d3d7:	00 
   9d3d8:	e9 ef d0 ff ff       	jmp    9a4cc <_Wfl.fl_vm_exec$a5081fe92876eb37+0xecc>
   9d3dd:	b8 6b ca eb 85       	mov    $0x85ebca6b,%eax
   9d3e2:	48 05 93 35 14 7a    	add    $0x7a143593,%rax
   9d3e8:	49 39 c3             	cmp    %rax,%r11
   9d3eb:	7e 34                	jle    9d421 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x3e21>
   9d3ed:	ba 0c 00 00 00       	mov    $0xc,%edx
   9d3f2:	41 b9 33 00 00 00    	mov    $0x33,%r9d
   9d3f8:	48 8b 7c 24 10       	mov    0x10(%rsp),%rdi
   9d3fd:	48 8d 35 5b 74 f7 ff 	lea    -0x88ba5(%rip),%rsi        # 1485f <__abi_tag+0x14563>
   9d404:	48 8b 4c 24 08       	mov    0x8(%rsp),%rcx
   9d409:	4c 8b 44 24 28       	mov    0x28(%rsp),%r8
   9d40e:	6a 1a                	push   $0x1a
   9d410:	48 8d 05 57 76 f7 ff 	lea    -0x889a9(%rip),%rax        # 14a6e <__abi_tag+0x14772>
   9d417:	50                   	push   %rax
   9d418:	e8 e3 89 05 00       	call   f5e00 <_Wfl.fl_vm_bug_at$494622488d4f7894>
   9d41d:	48 83 c4 10          	add    $0x10,%rsp
   9d421:	48 8b 44 24 10       	mov    0x10(%rsp),%rax
   9d426:	48 8b 80 b5 01 00 00 	mov    0x1b5(%rax),%rax
   9d42d:	48 ff c0             	inc    %rax
   9d430:	0f 81 9f 01 00 00    	jno    9d5d5 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x3fd5>
   9d436:	e9 94 17 00 00       	jmp    9ebcf <_Wfl.fl_vm_exec$a5081fe92876eb37+0x55cf>
   9d43b:	ba 0c 00 00 00       	mov    $0xc,%edx
   9d440:	4c 8b 74 24 10       	mov    0x10(%rsp),%r14
   9d445:	4c 89 f7             	mov    %r14,%rdi
   9d448:	48 8d 35 10 74 f7 ff 	lea    -0x88bf0(%rip),%rsi        # 1485f <__abi_tag+0x14563>
   9d44f:	48 8b 5c 24 08       	mov    0x8(%rsp),%rbx
   9d454:	48 89 d9             	mov    %rbx,%rcx
   9d457:	4c 8b 44 24 28       	mov    0x28(%rsp),%r8
   9d45c:	4c 8b 4c 24 20       	mov    0x20(%rsp),%r9
   9d461:	6a 15                	push   $0x15
   9d463:	48 8d 05 01 74 f7 ff 	lea    -0x88bff(%rip),%rax        # 1486b <__abi_tag+0x1456f>
   9d46a:	e9 97 03 00 00       	jmp    9d806 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4206>
   9d46f:	4c 8b 74 24 10       	mov    0x10(%rsp),%r14
   9d474:	4c 89 f7             	mov    %r14,%rdi
   9d477:	4c 89 ee             	mov    %r13,%rsi
   9d47a:	48 8b 54 24 08       	mov    0x8(%rsp),%rdx
   9d47f:	e8 8c 3e 02 00       	call   c1310 <_Wfl.fl_read8$a2ddcd4dfd790b8d>
   9d484:	48 89 c3             	mov    %rax,%rbx
   9d487:	4c 89 f7             	mov    %r14,%rdi
   9d48a:	48 8d b4 24 80 05 00 	lea    0x580(%rsp),%rsi
   9d491:	00 
   9d492:	e8 79 11 01 00       	call   ae610 <_Wfl.fl_pop$add7471f880d5ef3>
   9d497:	48 03 5c 24 40       	add    0x40(%rsp),%rbx
   9d49c:	0f 80 eb 16 00 00    	jo     9eb8d <_Wfl.fl_vm_exec$a5081fe92876eb37+0x558d>
   9d4a2:	48 8b 44 24 10       	mov    0x10(%rsp),%rax
   9d4a7:	48 8b 80 a4 00 00 00 	mov    0xa4(%rax),%rax
   9d4ae:	48 3b 58 08          	cmp    0x8(%rax),%rbx
   9d4b2:	0f 83 a3 18 00 00    	jae    9ed5b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x575b>
   9d4b8:	48 8b 00             	mov    (%rax),%rax
   9d4bb:	48 8d 0c 5b          	lea    (%rbx,%rbx,2),%rcx
   9d4bf:	f3 0f 7e 44 c8 10    	movq   0x10(%rax,%rcx,8),%xmm0
   9d4c5:	0f 10 0c c8          	movups (%rax,%rcx,8),%xmm1
   9d4c9:	0f 29 8c 24 40 03 00 	movaps %xmm1,0x340(%rsp)
   9d4d0:	00 
   9d4d1:	66 0f d6 84 24 50 03 	movq   %xmm0,0x350(%rsp)
   9d4d8:	00 00 
   9d4da:	4c 8b 74 24 10       	mov    0x10(%rsp),%r14
   9d4df:	4c 89 f7             	mov    %r14,%rdi
   9d4e2:	48 8d b4 24 40 03 00 	lea    0x340(%rsp),%rsi
   9d4e9:	00 
   9d4ea:	48 8d 94 24 80 05 00 	lea    0x580(%rsp),%rdx
   9d4f1:	00 
   9d4f2:	e8 89 40 05 00       	call   f1580 <_Wfl.fl_list_push$255051f5e64d80eb>
   9d4f7:	48 8b 6c 24 18       	mov    0x18(%rsp),%rbp
   9d4fc:	e9 1f db ff ff       	jmp    9b020 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1a20>
   9d501:	49 ff ca             	dec    %r10
   9d504:	0f 80 f1 16 00 00    	jo     9ebfb <_Wfl.fl_vm_exec$a5081fe92876eb37+0x55fb>
   9d50a:	4c 3b 56 08          	cmp    0x8(%rsi),%r10
   9d50e:	0f 83 1b 18 00 00    	jae    9ed2f <_Wfl.fl_vm_exec$a5081fe92876eb37+0x572f>
   9d514:	48 8b 06             	mov    (%rsi),%rax
   9d517:	4b 8d 0c 52          	lea    (%r10,%r10,2),%rcx
   9d51b:	31 d2                	xor    %edx,%edx
   9d51d:	48 83 3c c8 00       	cmpq   $0x0,(%rax,%rcx,8)
   9d522:	0f 95 c2             	setne  %dl
   9d525:	48 c7 04 c8 01 00 00 	movq   $0x1,(%rax,%rcx,8)
   9d52c:	00 
   9d52d:	48 89 54 c8 08       	mov    %rdx,0x8(%rax,%rcx,8)
   9d532:	48 c7 44 c8 10 00 00 	movq   $0x0,0x10(%rax,%rcx,8)
   9d539:	00 00 
   9d53b:	e9 ea 0f 00 00       	jmp    9e52a <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4f2a>
   9d540:	4c 8b 74 24 10       	mov    0x10(%rsp),%r14
   9d545:	4c 89 f7             	mov    %r14,%rdi
   9d548:	4c 89 ee             	mov    %r13,%rsi
   9d54b:	48 8b 54 24 08       	mov    0x8(%rsp),%rdx
   9d550:	e8 6b 3e 02 00       	call   c13c0 <_Wfl.fl_read16$7e28ba906e98be2a>
   9d555:	48 89 c3             	mov    %rax,%rbx
   9d558:	4d 8b ae cd 01 00 00 	mov    0x1cd(%r14),%r13
   9d55f:	49 8b be d5 01 00 00 	mov    0x1d5(%r14),%rdi
   9d566:	48 8b 47 08          	mov    0x8(%rdi),%rax
   9d56a:	48 3b 47 10          	cmp    0x10(%rdi),%rax
   9d56e:	0f 83 c2 04 00 00    	jae    9da36 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4436>
   9d574:	48 8b 0f             	mov    (%rdi),%rcx
   9d577:	48 89 1c c1          	mov    %rbx,(%rcx,%rax,8)
   9d57b:	48 ff c0             	inc    %rax
   9d57e:	48 89 47 08          	mov    %rax,0x8(%rdi)
   9d582:	e9 c4 04 00 00       	jmp    9da4b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x444b>
   9d587:	4d 85 db             	test   %r11,%r11
   9d58a:	7f 34                	jg     9d5c0 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x3fc0>
   9d58c:	ba 0c 00 00 00       	mov    $0xc,%edx
   9d591:	41 b9 34 00 00 00    	mov    $0x34,%r9d
   9d597:	48 8b 7c 24 10       	mov    0x10(%rsp),%rdi
   9d59c:	48 8d 35 bc 72 f7 ff 	lea    -0x88d44(%rip),%rsi        # 1485f <__abi_tag+0x14563>
   9d5a3:	48 8b 4c 24 08       	mov    0x8(%rsp),%rcx
   9d5a8:	4c 8b 44 24 28       	mov    0x28(%rsp),%r8
   9d5ad:	6a 1b                	push   $0x1b
   9d5af:	48 8d 05 d2 74 f7 ff 	lea    -0x88b2e(%rip),%rax        # 14a88 <__abi_tag+0x1478c>
   9d5b6:	50                   	push   %rax
   9d5b7:	e8 44 88 05 00       	call   f5e00 <_Wfl.fl_vm_bug_at$494622488d4f7894>
   9d5bc:	48 83 c4 10          	add    $0x10,%rsp
   9d5c0:	48 8b 44 24 10       	mov    0x10(%rsp),%rax
   9d5c5:	48 8b 80 b5 01 00 00 	mov    0x1b5(%rax),%rax
   9d5cc:	48 ff c8             	dec    %rax
   9d5cf:	0f 80 52 16 00 00    	jo     9ec27 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5627>
   9d5d5:	4c 8b 74 24 10       	mov    0x10(%rsp),%r14
   9d5da:	49 89 86 b5 01 00 00 	mov    %rax,0x1b5(%r14)
   9d5e1:	48 8b 6c 24 18       	mov    0x18(%rsp),%rbp
   9d5e6:	e9 35 da ff ff       	jmp    9b020 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1a20>
   9d5eb:	4c 8b 74 24 10       	mov    0x10(%rsp),%r14
   9d5f0:	4c 89 f7             	mov    %r14,%rdi
   9d5f3:	48 8d b4 24 60 03 00 	lea    0x360(%rsp),%rsi
   9d5fa:	00 
   9d5fb:	e8 10 10 01 00       	call   ae610 <_Wfl.fl_pop$add7471f880d5ef3>
   9d600:	0f 28 84 24 60 03 00 	movaps 0x360(%rsp),%xmm0
   9d607:	00 
   9d608:	41 0f 11 86 5d 01 00 	movups %xmm0,0x15d(%r14)
   9d60f:	00 
   9d610:	f3 0f 7e 84 24 70 03 	movq   0x370(%rsp),%xmm0
   9d617:	00 00 
   9d619:	66 41 0f d6 86 6d 01 	movq   %xmm0,0x16d(%r14)
   9d620:	00 00 
   9d622:	41 c6 86 5c 01 00 00 	movb   $0x1,0x15c(%r14)
   9d629:	01 
   9d62a:	48 8b 6c 24 18       	mov    0x18(%rsp),%rbp
   9d62f:	e9 ec d9 ff ff       	jmp    9b020 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1a20>
   9d634:	48 8b 5c 24 10       	mov    0x10(%rsp),%rbx
   9d639:	48 89 df             	mov    %rbx,%rdi
   9d63c:	4c 89 ee             	mov    %r13,%rsi
   9d63f:	48 8b 54 24 08       	mov    0x8(%rsp),%rdx
   9d644:	e8 77 3d 02 00       	call   c13c0 <_Wfl.fl_read16$7e28ba906e98be2a>
   9d649:	48 8b bb bc 00 00 00 	mov    0xbc(%rbx),%rdi
   9d650:	48 8b 4f 08          	mov    0x8(%rdi),%rcx
   9d654:	48 83 f9 40          	cmp    $0x40,%rcx
   9d658:	0f 8c fc 03 00 00    	jl     9da5a <_Wfl.fl_vm_exec$a5081fe92876eb37+0x445a>
   9d65e:	ba 05 00 00 00       	mov    $0x5,%edx
   9d663:	41 b8 1a 00 00 00    	mov    $0x1a,%r8d
   9d669:	4c 8b 74 24 10       	mov    0x10(%rsp),%r14
   9d66e:	4c 89 f7             	mov    %r14,%rdi
   9d671:	48 8d 35 6f 6f f7 ff 	lea    -0x89091(%rip),%rsi        # 145e7 <__abi_tag+0x142eb>
   9d678:	48 8d 0d 24 74 f7 ff 	lea    -0x88bdc(%rip),%rcx        # 14aa3 <__abi_tag+0x147a7>
   9d67f:	4c 8d 8c 24 f8 1f 00 	lea    0x1ff8(%rsp),%r9
   9d686:	00 
   9d687:	e9 83 f1 ff ff       	jmp    9c80f <_Wfl.fl_vm_exec$a5081fe92876eb37+0x320f>
   9d68c:	49 8b 47 08          	mov    0x8(%r15),%rax
   9d690:	48 85 c0             	test   %rax,%rax
   9d693:	0f 8e a8 04 00 00    	jle    9db41 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4541>
   9d699:	48 ff c8             	dec    %rax
   9d69c:	49 89 47 08          	mov    %rax,0x8(%r15)
   9d6a0:	e9 85 0e 00 00       	jmp    9e52a <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4f2a>
   9d6a5:	b8 6b ca eb 85       	mov    $0x85ebca6b,%eax
   9d6aa:	48 8d 88 4e af 4b 18 	lea    0x184baf4e(%rax),%rcx
   9d6b1:	48 83 7c 24 38 00    	cmpq   $0x0,0x38(%rsp)
   9d6b7:	48 0f 44 c8          	cmove  %rax,%rcx
   9d6bb:	e9 80 00 00 00       	jmp    9d740 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4140>
   9d6c0:	48 8b 54 24 38       	mov    0x38(%rsp),%rdx
   9d6c5:	0f b6 c2             	movzbl %dl,%eax
   9d6c8:	35 c5 9d 1c 81       	xor    $0x811c9dc5,%eax
   9d6cd:	69 c0 93 01 00 01    	imul   $0x1000193,%eax,%eax
   9d6d3:	0f b6 ce             	movzbl %dh,%ecx
   9d6d6:	31 c1                	xor    %eax,%ecx
   9d6d8:	69 c1 93 01 00 01    	imul   $0x1000193,%ecx,%eax
   9d6de:	89 d1                	mov    %edx,%ecx
   9d6e0:	c1 e9 10             	shr    $0x10,%ecx
   9d6e3:	0f b6 c9             	movzbl %cl,%ecx
   9d6e6:	31 c1                	xor    %eax,%ecx
   9d6e8:	69 c1 93 01 00 01    	imul   $0x1000193,%ecx,%eax
   9d6ee:	89 d1                	mov    %edx,%ecx
   9d6f0:	c1 e9 18             	shr    $0x18,%ecx
   9d6f3:	31 c1                	xor    %eax,%ecx
   9d6f5:	69 c1 93 01 00 01    	imul   $0x1000193,%ecx,%eax
   9d6fb:	48 89 d1             	mov    %rdx,%rcx
   9d6fe:	48 c1 e9 20          	shr    $0x20,%rcx
   9d702:	0f b6 c9             	movzbl %cl,%ecx
   9d705:	31 c1                	xor    %eax,%ecx
   9d707:	69 c1 93 01 00 01    	imul   $0x1000193,%ecx,%eax
   9d70d:	48 89 d1             	mov    %rdx,%rcx
   9d710:	48 c1 e9 28          	shr    $0x28,%rcx
   9d714:	0f b6 c9             	movzbl %cl,%ecx
   9d717:	31 c1                	xor    %eax,%ecx
   9d719:	69 c1 93 01 00 01    	imul   $0x1000193,%ecx,%eax
   9d71f:	48 89 d1             	mov    %rdx,%rcx
   9d722:	48 c1 e9 30          	shr    $0x30,%rcx
   9d726:	0f b6 c9             	movzbl %cl,%ecx
   9d729:	31 c1                	xor    %eax,%ecx
   9d72b:	69 c1 93 01 00 01    	imul   $0x1000193,%ecx,%eax
   9d731:	48 89 d1             	mov    %rdx,%rcx
   9d734:	48 c1 e9 38          	shr    $0x38,%rcx
   9d738:	31 c1                	xor    %eax,%ecx
   9d73a:	69 c9 93 01 00 01    	imul   $0x1000193,%ecx,%ecx
   9d740:	48 8d bc 24 10 06 00 	lea    0x610(%rsp),%rdi
   9d747:	00 
   9d748:	48 89 de             	mov    %rbx,%rsi
   9d74b:	48 8d 94 24 88 00 00 	lea    0x88(%rsp),%rdx
   9d752:	00 
   9d753:	e8 78 f0 07 00       	call   11c7d0 <_Wfl.fl_map_probe$234ad4c981fd6647>
   9d758:	84 d2                	test   %dl,%dl
   9d75a:	0f 84 ba 00 00 00    	je     9d81a <_Wfl.fl_vm_exec$a5081fe92876eb37+0x421a>
   9d760:	48 3b 5d 08          	cmp    0x8(%rbp),%rbx
   9d764:	0f 83 6d 15 00 00    	jae    9ecd7 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x56d7>
   9d76a:	48 8b 4d 00          	mov    0x0(%rbp),%rcx
   9d76e:	4a 8b 54 21 39       	mov    0x39(%rcx,%r12,1),%rdx
   9d773:	48 3b 42 08          	cmp    0x8(%rdx),%rax
   9d777:	0f 83 5a 15 00 00    	jae    9ecd7 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x56d7>
   9d77d:	48 8b 12             	mov    (%rdx),%rdx
   9d780:	48 8b 04 c2          	mov    (%rdx,%rax,8),%rax
   9d784:	48 ff c8             	dec    %rax
   9d787:	0f 80 d4 13 00 00    	jo     9eb61 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5561>
   9d78d:	4a 8b 4c 21 31       	mov    0x31(%rcx,%r12,1),%rcx
   9d792:	48 3b 41 08          	cmp    0x8(%rcx),%rax
   9d796:	0f 83 3b 15 00 00    	jae    9ecd7 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x56d7>
   9d79c:	4c 8b 74 24 10       	mov    0x10(%rsp),%r14
   9d7a1:	4c 89 f7             	mov    %r14,%rdi
   9d7a4:	48 8d 9c 24 c0 21 00 	lea    0x21c0(%rsp),%rbx
   9d7ab:	00 
   9d7ac:	48 89 de             	mov    %rbx,%rsi
   9d7af:	e8 5c 0e 01 00       	call   ae610 <_Wfl.fl_pop$add7471f880d5ef3>
   9d7b4:	4c 89 f7             	mov    %r14,%rdi
   9d7b7:	48 8d b4 24 50 01 00 	lea    0x150(%rsp),%rsi
   9d7be:	00 
   9d7bf:	48 8d 94 24 88 00 00 	lea    0x88(%rsp),%rdx
   9d7c6:	00 
   9d7c7:	48 89 d9             	mov    %rbx,%rcx
   9d7ca:	e8 21 5e ff ff       	call   935f0 <_Wfl.fl_map_set$540fcc92758450a0>
   9d7cf:	84 c0                	test   %al,%al
   9d7d1:	0f 85 53 0d 00 00    	jne    9e52a <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4f2a>
   9d7d7:	ba 0c 00 00 00       	mov    $0xc,%edx
   9d7dc:	4c 8b 74 24 10       	mov    0x10(%rsp),%r14
   9d7e1:	4c 89 f7             	mov    %r14,%rdi
   9d7e4:	48 8d 35 74 70 f7 ff 	lea    -0x88f8c(%rip),%rsi        # 1485f <__abi_tag+0x14563>
   9d7eb:	48 8b 5c 24 08       	mov    0x8(%rsp),%rbx
   9d7f0:	48 89 d9             	mov    %rbx,%rcx
   9d7f3:	4c 8b 44 24 28       	mov    0x28(%rsp),%r8
   9d7f8:	4c 8b 4c 24 20       	mov    0x20(%rsp),%r9
   9d7fd:	6a 1e                	push   $0x1e
   9d7ff:	48 8d 05 51 71 f7 ff 	lea    -0x88eaf(%rip),%rax        # 14957 <__abi_tag+0x1465b>
   9d806:	50                   	push   %rax
   9d807:	e8 f4 85 05 00       	call   f5e00 <_Wfl.fl_vm_bug_at$494622488d4f7894>
   9d80c:	48 83 c4 10          	add    $0x10,%rsp
   9d810:	48 8b 6c 24 18       	mov    0x18(%rsp),%rbp
   9d815:	e9 0b d8 ff ff       	jmp    9b025 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1a25>
   9d81a:	48 8b 44 24 38       	mov    0x38(%rsp),%rax
   9d81f:	49 3b 47 08          	cmp    0x8(%r15),%rax
   9d823:	0f 83 c6 16 00 00    	jae    9eeef <_Wfl.fl_vm_exec$a5081fe92876eb37+0x58ef>
   9d829:	49 8b 07             	mov    (%r15),%rax
   9d82c:	48 8b 4c 24 38       	mov    0x38(%rsp),%rcx
   9d831:	48 8b 3c c8          	mov    (%rax,%rcx,8),%rdi
   9d835:	48 8d b4 24 f0 01 00 	lea    0x1f0(%rsp),%rsi
   9d83c:	00 
   9d83d:	e8 1e a5 0c 00       	call   167d60 <__wolf_rt_str_from_utf8>
   9d842:	48 85 c0             	test   %rax,%rax
   9d845:	0f 85 71 0a 00 00    	jne    9e2bc <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4cbc>
   9d84b:	4c 8b ac 24 f0 01 00 	mov    0x1f0(%rsp),%r13
   9d852:	00 
   9d853:	48 8b 9c 24 f8 01 00 	mov    0x1f8(%rsp),%rbx
   9d85a:	00 
   9d85b:	e8 c0 ac 0c 00       	call   168520 <__wolf_rt_strbuf_new>
   9d860:	48 89 c5             	mov    %rax,%rbp
   9d863:	ba 10 00 00 00       	mov    $0x10,%edx
   9d868:	48 89 c7             	mov    %rax,%rdi
   9d86b:	48 8d 35 1e 13 f8 ff 	lea    -0x7ece2(%rip),%rsi        # 1eb90 <__abi_tag+0x1e894>
   9d872:	31 c9                	xor    %ecx,%ecx
   9d874:	e8 57 ad 0c 00       	call   1685d0 <__wolf_rt_strbuf_str>
   9d879:	48 89 ef             	mov    %rbp,%rdi
   9d87c:	4c 89 ee             	mov    %r13,%rsi
   9d87f:	48 89 da             	mov    %rbx,%rdx
   9d882:	31 c9                	xor    %ecx,%ecx
   9d884:	e8 47 ad 0c 00       	call   1685d0 <__wolf_rt_strbuf_str>
   9d889:	ba 01 00 00 00       	mov    $0x1,%edx
   9d88e:	48 89 ef             	mov    %rbp,%rdi
   9d891:	48 8d 35 be 70 f7 ff 	lea    -0x88f42(%rip),%rsi        # 14956 <__abi_tag+0x1465a>
   9d898:	31 c9                	xor    %ecx,%ecx
   9d89a:	e8 31 ad 0c 00       	call   1685d0 <__wolf_rt_strbuf_str>
   9d89f:	48 89 ef             	mov    %rbp,%rdi
   9d8a2:	48 8d b4 24 e0 01 00 	lea    0x1e0(%rsp),%rsi
   9d8a9:	00 
   9d8aa:	e8 21 a9 0c 00       	call   1681d0 <__wolf_rt_strbuf_finish>
   9d8af:	48 8b 8c 24 e0 01 00 	mov    0x1e0(%rsp),%rcx
   9d8b6:	00 
   9d8b7:	4c 8b 84 24 e8 01 00 	mov    0x1e8(%rsp),%r8
   9d8be:	00 
   9d8bf:	49 81 f8 80 00 00 00 	cmp    $0x80,%r8
   9d8c6:	7c 19                	jl     9d8e1 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x42e1>
   9d8c8:	80 39 c0             	cmpb   $0xc0,(%rcx)
   9d8cb:	0f 8c 1e 16 00 00    	jl     9eeef <_Wfl.fl_vm_exec$a5081fe92876eb37+0x58ef>
   9d8d1:	41 b8 7f 00 00 00    	mov    $0x7f,%r8d
   9d8d7:	80 79 7f bf          	cmpb   $0xbf,0x7f(%rcx)
   9d8db:	0f 8e 0e 16 00 00    	jle    9eeef <_Wfl.fl_vm_exec$a5081fe92876eb37+0x58ef>
   9d8e1:	ba 04 00 00 00       	mov    $0x4,%edx
   9d8e6:	4c 8b 74 24 10       	mov    0x10(%rsp),%r14
   9d8eb:	4c 89 f7             	mov    %r14,%rdi
   9d8ee:	48 8d 35 c3 17 f8 ff 	lea    -0x7e83d(%rip),%rsi        # 1f0b8 <__abi_tag+0x1edbc>
   9d8f5:	4c 8d 8c 24 d8 21 00 	lea    0x21d8(%rsp),%r9
   9d8fc:	00 
   9d8fd:	e9 0d ef ff ff       	jmp    9c80f <_Wfl.fl_vm_exec$a5081fe92876eb37+0x320f>
   9d902:	48 8b 54 24 38       	mov    0x38(%rsp),%rdx
   9d907:	0f b6 c2             	movzbl %dl,%eax
   9d90a:	35 c5 9d 1c 81       	xor    $0x811c9dc5,%eax
   9d90f:	69 c0 93 01 00 01    	imul   $0x1000193,%eax,%eax
   9d915:	0f b6 ce             	movzbl %dh,%ecx
   9d918:	31 c1                	xor    %eax,%ecx
   9d91a:	69 c1 93 01 00 01    	imul   $0x1000193,%ecx,%eax
   9d920:	89 d1                	mov    %edx,%ecx
   9d922:	c1 e9 10             	shr    $0x10,%ecx
   9d925:	0f b6 c9             	movzbl %cl,%ecx
   9d928:	31 c1                	xor    %eax,%ecx
   9d92a:	69 c1 93 01 00 01    	imul   $0x1000193,%ecx,%eax
   9d930:	89 d1                	mov    %edx,%ecx
   9d932:	c1 e9 18             	shr    $0x18,%ecx
   9d935:	31 c1                	xor    %eax,%ecx
   9d937:	69 c1 93 01 00 01    	imul   $0x1000193,%ecx,%eax
   9d93d:	48 89 d1             	mov    %rdx,%rcx
   9d940:	48 c1 e9 20          	shr    $0x20,%rcx
   9d944:	0f b6 c9             	movzbl %cl,%ecx
   9d947:	31 c1                	xor    %eax,%ecx
   9d949:	69 c1 93 01 00 01    	imul   $0x1000193,%ecx,%eax
   9d94f:	48 89 d1             	mov    %rdx,%rcx
   9d952:	48 c1 e9 28          	shr    $0x28,%rcx
   9d956:	0f b6 c9             	movzbl %cl,%ecx
   9d959:	31 c1                	xor    %eax,%ecx
   9d95b:	69 c1 93 01 00 01    	imul   $0x1000193,%ecx,%eax
   9d961:	48 89 d1             	mov    %rdx,%rcx
   9d964:	48 c1 e9 30          	shr    $0x30,%rcx
   9d968:	0f b6 c9             	movzbl %cl,%ecx
   9d96b:	31 c1                	xor    %eax,%ecx
   9d96d:	69 c1 93 01 00 01    	imul   $0x1000193,%ecx,%eax
   9d973:	48 89 d1             	mov    %rdx,%rcx
   9d976:	48 c1 e9 38          	shr    $0x38,%rcx
   9d97a:	31 c1                	xor    %eax,%ecx
   9d97c:	69 c9 93 01 00 01    	imul   $0x1000193,%ecx,%ecx
   9d982:	48 8d bc 24 10 06 00 	lea    0x610(%rsp),%rdi
   9d989:	00 
   9d98a:	48 89 de             	mov    %rbx,%rsi
   9d98d:	48 8d 94 24 88 00 00 	lea    0x88(%rsp),%rdx
   9d994:	00 
   9d995:	e8 36 ee 07 00       	call   11c7d0 <_Wfl.fl_map_probe$234ad4c981fd6647>
   9d99a:	84 d2                	test   %dl,%dl
   9d99c:	0f 84 71 08 00 00    	je     9e213 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4c13>
   9d9a2:	48 3b 5d 08          	cmp    0x8(%rbp),%rbx
   9d9a6:	0f 83 41 13 00 00    	jae    9eced <_Wfl.fl_vm_exec$a5081fe92876eb37+0x56ed>
   9d9ac:	48 8b 55 00          	mov    0x0(%rbp),%rdx
   9d9b0:	4a 8b 4c 3a 39       	mov    0x39(%rdx,%r15,1),%rcx
   9d9b5:	48 3b 41 08          	cmp    0x8(%rcx),%rax
   9d9b9:	0f 83 2e 13 00 00    	jae    9eced <_Wfl.fl_vm_exec$a5081fe92876eb37+0x56ed>
   9d9bf:	48 8b 09             	mov    (%rcx),%rcx
   9d9c2:	48 8b 0c c1          	mov    (%rcx,%rax,8),%rcx
   9d9c6:	48 ff c9             	dec    %rcx
   9d9c9:	0f 80 3a 11 00 00    	jo     9eb09 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5509>
   9d9cf:	4a 8b 44 3a 31       	mov    0x31(%rdx,%r15,1),%rax
   9d9d4:	48 3b 48 08          	cmp    0x8(%rax),%rcx
   9d9d8:	0f 83 0f 13 00 00    	jae    9eced <_Wfl.fl_vm_exec$a5081fe92876eb37+0x56ed>
   9d9de:	48 8b 10             	mov    (%rax),%rdx
   9d9e1:	48 c1 e1 06          	shl    $0x6,%rcx
   9d9e5:	48 8d 04 0a          	lea    (%rdx,%rcx,1),%rax
   9d9e9:	48 83 c0 18          	add    $0x18,%rax
   9d9ed:	66 0f 6f 44 0a 20    	movdqa 0x20(%rdx,%rcx,1),%xmm0
   9d9f3:	e9 0e 0b 00 00       	jmp    9e506 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4f06>
   9d9f8:	48 8b 00             	mov    (%rax),%rax
   9d9fb:	4a 8d 0c 6d 00 00 00 	lea    0x0(,%r13,2),%rcx
   9da02:	00 
   9da03:	4c 01 e9             	add    %r13,%rcx
   9da06:	48 8b 54 c8 10       	mov    0x10(%rax,%rcx,8),%rdx
   9da0b:	f3 0f 6f 04 c8       	movdqu (%rax,%rcx,8),%xmm0
   9da10:	66 0f 7f 84 24 80 04 	movdqa %xmm0,0x480(%rsp)
   9da17:	00 00 
   9da19:	48 89 94 24 90 04 00 	mov    %rdx,0x490(%rsp)
   9da20:	00 
   9da21:	4c 8b 74 24 10       	mov    0x10(%rsp),%r14
   9da26:	4c 89 f7             	mov    %r14,%rdi
   9da29:	48 8d b4 24 80 04 00 	lea    0x480(%rsp),%rsi
   9da30:	00 
   9da31:	e9 96 ca ff ff       	jmp    9a4cc <_Wfl.fl_vm_exec$a5081fe92876eb37+0xecc>
   9da36:	48 89 9c 24 00 01 00 	mov    %rbx,0x100(%rsp)
   9da3d:	00 
   9da3e:	48 8d b4 24 00 01 00 	lea    0x100(%rsp),%rsi
   9da45:	00 
   9da46:	e8 b5 05 0c 00       	call   15e000 <__wolf_rt_list_push>
   9da4b:	4c 89 ef             	mov    %r13,%rdi
   9da4e:	48 89 de             	mov    %rbx,%rsi
   9da51:	41 ff 55 00          	call   *0x0(%r13)
   9da55:	e9 d0 0a 00 00       	jmp    9e52a <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4f2a>
   9da5a:	48 8b 54 24 10       	mov    0x10(%rsp),%rdx
   9da5f:	48 8b b2 b4 00 00 00 	mov    0xb4(%rdx),%rsi
   9da66:	48 8b 56 08          	mov    0x8(%rsi),%rdx
   9da6a:	49 39 d5             	cmp    %rdx,%r13
   9da6d:	0f 83 d7 13 00 00    	jae    9ee4a <_Wfl.fl_vm_exec$a5081fe92876eb37+0x584a>
   9da73:	48 8b 36             	mov    (%rsi),%rsi
   9da76:	4c 8b 44 24 38       	mov    0x38(%rsp),%r8
   9da7b:	4a 03 44 06 18       	add    0x18(%rsi,%r8,1),%rax
   9da80:	0f 80 32 14 00 00    	jo     9eeb8 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x58b8>
   9da86:	48 8b 74 24 10       	mov    0x10(%rsp),%rsi
   9da8b:	4c 8b 86 ac 00 00 00 	mov    0xac(%rsi),%r8
   9da92:	48 8b b6 b5 01 00 00 	mov    0x1b5(%rsi),%rsi
   9da99:	48 3b 4f 10          	cmp    0x10(%rdi),%rcx
   9da9d:	0f 83 d3 00 00 00    	jae    9db76 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4576>
   9daa3:	4c 8b 0f             	mov    (%rdi),%r9
   9daa6:	49 89 ca             	mov    %rcx,%r10
   9daa9:	49 c1 e2 05          	shl    $0x5,%r10
   9daad:	4b 89 04 11          	mov    %rax,(%r9,%r10,1)
   9dab1:	4f 89 44 11 08       	mov    %r8,0x8(%r9,%r10,1)
   9dab6:	4b 89 54 11 10       	mov    %rdx,0x10(%r9,%r10,1)
   9dabb:	4b 89 74 11 18       	mov    %rsi,0x18(%r9,%r10,1)
   9dac0:	48 ff c1             	inc    %rcx
   9dac3:	48 89 4f 08          	mov    %rcx,0x8(%rdi)
   9dac7:	e9 5e 0a 00 00       	jmp    9e52a <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4f2a>
   9dacc:	31 db                	xor    %ebx,%ebx
   9dace:	4c 8b 74 24 10       	mov    0x10(%rsp),%r14
   9dad3:	4c 89 f7             	mov    %r14,%rdi
   9dad6:	48 8d b4 24 08 05 00 	lea    0x508(%rsp),%rsi
   9dadd:	00 
   9dade:	e8 fd 19 03 00       	call   cf4e0 <_Wfl.fl_push$396d177d8e34f937>
   9dae3:	48 c7 84 24 40 02 00 	movq   $0x2,0x240(%rsp)
   9daea:	00 02 00 00 00 
   9daef:	48 89 9c 24 48 02 00 	mov    %rbx,0x248(%rsp)
   9daf6:	00 
   9daf7:	48 c7 84 24 50 02 00 	movq   $0x0,0x250(%rsp)
   9dafe:	00 00 00 00 00 
   9db03:	4c 89 f7             	mov    %r14,%rdi
   9db06:	48 8d b4 24 40 02 00 	lea    0x240(%rsp),%rsi
   9db0d:	00 
   9db0e:	e9 b9 c9 ff ff       	jmp    9a4cc <_Wfl.fl_vm_exec$a5081fe92876eb37+0xecc>
   9db13:	ba 04 00 00 00       	mov    $0x4,%edx
   9db18:	41 b8 10 00 00 00    	mov    $0x10,%r8d
   9db1e:	4c 8b 74 24 10       	mov    0x10(%rsp),%r14
   9db23:	4c 89 f7             	mov    %r14,%rdi
   9db26:	48 8d 35 b3 15 f8 ff 	lea    -0x7ea4d(%rip),%rsi        # 1f0e0 <__abi_tag+0x1ede4>
   9db2d:	48 8d 0d fc 0e f8 ff 	lea    -0x7f104(%rip),%rcx        # 1ea30 <__abi_tag+0x1e734>
   9db34:	4c 8d 8c 24 40 20 00 	lea    0x2040(%rsp),%r9
   9db3b:	00 
   9db3c:	e9 ce ec ff ff       	jmp    9c80f <_Wfl.fl_vm_exec$a5081fe92876eb37+0x320f>
   9db41:	ba 0c 00 00 00       	mov    $0xc,%edx
   9db46:	41 b9 37 00 00 00    	mov    $0x37,%r9d
   9db4c:	4c 8b 74 24 10       	mov    0x10(%rsp),%r14
   9db51:	4c 89 f7             	mov    %r14,%rdi
   9db54:	48 8d 35 04 6d f7 ff 	lea    -0x892fc(%rip),%rsi        # 1485f <__abi_tag+0x14563>
   9db5b:	48 8b 5c 24 08       	mov    0x8(%rsp),%rbx
   9db60:	48 89 d9             	mov    %rbx,%rcx
   9db63:	4c 8b 44 24 28       	mov    0x28(%rsp),%r8
   9db68:	6a 1e                	push   $0x1e
   9db6a:	48 8d 05 4c 6f f7 ff 	lea    -0x890b4(%rip),%rax        # 14abd <__abi_tag+0x147c1>
   9db71:	e9 90 fc ff ff       	jmp    9d806 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4206>
   9db76:	48 89 84 24 00 03 00 	mov    %rax,0x300(%rsp)
   9db7d:	00 
   9db7e:	4c 89 84 24 08 03 00 	mov    %r8,0x308(%rsp)
   9db85:	00 
   9db86:	48 89 94 24 10 03 00 	mov    %rdx,0x310(%rsp)
   9db8d:	00 
   9db8e:	48 89 b4 24 18 03 00 	mov    %rsi,0x318(%rsp)
   9db95:	00 
   9db96:	48 8d b4 24 00 03 00 	lea    0x300(%rsp),%rsi
   9db9d:	00 
   9db9e:	e8 5d 04 0c 00       	call   15e000 <__wolf_rt_list_push>
   9dba3:	e9 82 09 00 00       	jmp    9e52a <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4f2a>
   9dba8:	bd 0f 00 00 00       	mov    $0xf,%ebp
   9dbad:	4c 8d 2d ef 6b f7 ff 	lea    -0x89411(%rip),%r13        # 147a3 <__abi_tag+0x144a7>
   9dbb4:	e9 8e e4 ff ff       	jmp    9c047 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x2a47>
   9dbb9:	66 0f 6f 44 24 50    	movdqa 0x50(%rsp),%xmm0
   9dbbf:	48 8b 44 24 60       	mov    0x60(%rsp),%rax
   9dbc4:	e9 dd e4 ff ff       	jmp    9c0a6 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x2aa6>
   9dbc9:	48 8b 54 24 10       	mov    0x10(%rsp),%rdx
   9dbce:	48 8b 4a 20          	mov    0x20(%rdx),%rcx
   9dbd2:	48 8b 42 48          	mov    0x48(%rdx),%rax
   9dbd6:	0f 10 02             	movups (%rdx),%xmm0
   9dbd9:	0f 29 84 24 40 0b 00 	movaps %xmm0,0xb40(%rsp)
   9dbe0:	00 
   9dbe1:	0f 10 42 10          	movups 0x10(%rdx),%xmm0
   9dbe5:	0f 29 84 24 50 0b 00 	movaps %xmm0,0xb50(%rsp)
   9dbec:	00 
   9dbed:	48 89 8c 24 60 0b 00 	mov    %rcx,0xb60(%rsp)
   9dbf4:	00 
   9dbf5:	0f b7 4a 50          	movzwl 0x50(%rdx),%ecx
   9dbf9:	0f 10 42 28          	movups 0x28(%rdx),%xmm0
   9dbfd:	0f 11 84 24 68 0b 00 	movups %xmm0,0xb68(%rsp)
   9dc04:	00 
   9dc05:	0f 10 42 38          	movups 0x38(%rdx),%xmm0
   9dc09:	0f 11 84 24 78 0b 00 	movups %xmm0,0xb78(%rsp)
   9dc10:	00 
   9dc11:	48 89 84 24 88 0b 00 	mov    %rax,0xb88(%rsp)
   9dc18:	00 
   9dc19:	48 8b 42 62          	mov    0x62(%rdx),%rax
   9dc1d:	48 89 8c 24 90 0b 00 	mov    %rcx,0xb90(%rsp)
   9dc24:	00 
   9dc25:	48 8b 8a 8a 00 00 00 	mov    0x8a(%rdx),%rcx
   9dc2c:	0f 10 42 52          	movups 0x52(%rdx),%xmm0
   9dc30:	0f 11 84 24 98 0b 00 	movups %xmm0,0xb98(%rsp)
   9dc37:	00 
   9dc38:	48 89 84 24 a8 0b 00 	mov    %rax,0xba8(%rsp)
   9dc3f:	00 
   9dc40:	0f b7 82 a2 00 00 00 	movzwl 0xa2(%rdx),%eax
   9dc47:	0f 10 42 6a          	movups 0x6a(%rdx),%xmm0
   9dc4b:	0f 29 84 24 b0 0b 00 	movaps %xmm0,0xbb0(%rsp)
   9dc52:	00 
   9dc53:	0f 10 42 7a          	movups 0x7a(%rdx),%xmm0
   9dc57:	0f 29 84 24 c0 0b 00 	movaps %xmm0,0xbc0(%rsp)
   9dc5e:	00 
   9dc5f:	0f 10 82 92 00 00 00 	movups 0x92(%rdx),%xmm0
   9dc66:	48 89 8c 24 d0 0b 00 	mov    %rcx,0xbd0(%rsp)
   9dc6d:	00 
   9dc6e:	0f 11 84 24 d8 0b 00 	movups %xmm0,0xbd8(%rsp)
   9dc75:	00 
   9dc76:	0f 10 82 a4 00 00 00 	movups 0xa4(%rdx),%xmm0
   9dc7d:	48 8b 8a c4 00 00 00 	mov    0xc4(%rdx),%rcx
   9dc84:	48 89 84 24 e8 0b 00 	mov    %rax,0xbe8(%rsp)
   9dc8b:	00 
   9dc8c:	0f b6 82 5c 01 00 00 	movzbl 0x15c(%rdx),%eax
   9dc93:	0f 29 84 24 f0 0b 00 	movaps %xmm0,0xbf0(%rsp)
   9dc9a:	00 
   9dc9b:	0f 10 82 b4 00 00 00 	movups 0xb4(%rdx),%xmm0
   9dca2:	0f 29 84 24 00 0c 00 	movaps %xmm0,0xc00(%rsp)
   9dca9:	00 
   9dcaa:	0f 10 82 cc 00 00 00 	movups 0xcc(%rdx),%xmm0
   9dcb1:	48 89 8c 24 10 0c 00 	mov    %rcx,0xc10(%rsp)
   9dcb8:	00 
   9dcb9:	0f 11 84 24 18 0c 00 	movups %xmm0,0xc18(%rsp)
   9dcc0:	00 
   9dcc1:	0f 10 82 dc 00 00 00 	movups 0xdc(%rdx),%xmm0
   9dcc8:	0f 11 84 24 28 0c 00 	movups %xmm0,0xc28(%rsp)
   9dccf:	00 
   9dcd0:	0f 10 82 ec 00 00 00 	movups 0xec(%rdx),%xmm0
   9dcd7:	0f 11 84 24 38 0c 00 	movups %xmm0,0xc38(%rsp)
   9dcde:	00 
   9dcdf:	0f 10 82 fc 00 00 00 	movups 0xfc(%rdx),%xmm0
   9dce6:	0f 11 84 24 48 0c 00 	movups %xmm0,0xc48(%rsp)
   9dced:	00 
   9dcee:	0f 10 82 0c 01 00 00 	movups 0x10c(%rdx),%xmm0
   9dcf5:	0f 11 84 24 58 0c 00 	movups %xmm0,0xc58(%rsp)
   9dcfc:	00 
   9dcfd:	0f 10 82 1c 01 00 00 	movups 0x11c(%rdx),%xmm0
   9dd04:	0f 11 84 24 68 0c 00 	movups %xmm0,0xc68(%rsp)
   9dd0b:	00 
   9dd0c:	0f 10 82 2c 01 00 00 	movups 0x12c(%rdx),%xmm0
   9dd13:	0f 11 84 24 78 0c 00 	movups %xmm0,0xc78(%rsp)
   9dd1a:	00 
   9dd1b:	0f 10 82 3c 01 00 00 	movups 0x13c(%rdx),%xmm0
   9dd22:	0f 11 84 24 88 0c 00 	movups %xmm0,0xc88(%rsp)
   9dd29:	00 
   9dd2a:	0f 10 82 4c 01 00 00 	movups 0x14c(%rdx),%xmm0
   9dd31:	0f 11 84 24 98 0c 00 	movups %xmm0,0xc98(%rsp)
   9dd38:	00 
   9dd39:	0f 10 82 5d 01 00 00 	movups 0x15d(%rdx),%xmm0
   9dd40:	88 84 24 a8 0c 00 00 	mov    %al,0xca8(%rsp)
   9dd47:	0f 29 84 24 b0 0c 00 	movaps %xmm0,0xcb0(%rsp)
   9dd4e:	00 
   9dd4f:	0f 10 82 6d 01 00 00 	movups 0x16d(%rdx),%xmm0
   9dd56:	0f b6 82 dd 01 00 00 	movzbl 0x1dd(%rdx),%eax
   9dd5d:	48 8b 8a ee 01 00 00 	mov    0x1ee(%rdx),%rcx
   9dd64:	0f 29 84 24 c0 0c 00 	movaps %xmm0,0xcc0(%rsp)
   9dd6b:	00 
   9dd6c:	0f 10 82 7d 01 00 00 	movups 0x17d(%rdx),%xmm0
   9dd73:	0f 29 84 24 d0 0c 00 	movaps %xmm0,0xcd0(%rsp)
   9dd7a:	00 
   9dd7b:	0f 10 82 8d 01 00 00 	movups 0x18d(%rdx),%xmm0
   9dd82:	0f 29 84 24 e0 0c 00 	movaps %xmm0,0xce0(%rsp)
   9dd89:	00 
   9dd8a:	0f 10 82 9d 01 00 00 	movups 0x19d(%rdx),%xmm0
   9dd91:	0f 29 84 24 f0 0c 00 	movaps %xmm0,0xcf0(%rsp)
   9dd98:	00 
   9dd99:	0f 10 82 ad 01 00 00 	movups 0x1ad(%rdx),%xmm0
   9dda0:	0f 29 84 24 00 0d 00 	movaps %xmm0,0xd00(%rsp)
   9dda7:	00 
   9dda8:	0f 10 82 bd 01 00 00 	movups 0x1bd(%rdx),%xmm0
   9ddaf:	0f 29 84 24 10 0d 00 	movaps %xmm0,0xd10(%rsp)
   9ddb6:	00 
   9ddb7:	0f 10 82 cd 01 00 00 	movups 0x1cd(%rdx),%xmm0
   9ddbe:	0f 29 84 24 20 0d 00 	movaps %xmm0,0xd20(%rsp)
   9ddc5:	00 
   9ddc6:	88 84 24 30 0d 00 00 	mov    %al,0xd30(%rsp)
   9ddcd:	48 8b 82 16 02 00 00 	mov    0x216(%rdx),%rax
   9ddd4:	0f 10 82 de 01 00 00 	movups 0x1de(%rdx),%xmm0
   9dddb:	0f 11 84 24 38 0d 00 	movups %xmm0,0xd38(%rsp)
   9dde2:	00 
   9dde3:	48 89 8c 24 48 0d 00 	mov    %rcx,0xd48(%rsp)
   9ddea:	00 
   9ddeb:	0f b6 8a 1e 02 00 00 	movzbl 0x21e(%rdx),%ecx
   9ddf2:	0f 10 82 f6 01 00 00 	movups 0x1f6(%rdx),%xmm0
   9ddf9:	0f 29 84 24 50 0d 00 	movaps %xmm0,0xd50(%rsp)
   9de00:	00 
   9de01:	0f 10 82 06 02 00 00 	movups 0x206(%rdx),%xmm0
   9de08:	0f 29 84 24 60 0d 00 	movaps %xmm0,0xd60(%rsp)
   9de0f:	00 
   9de10:	48 89 84 24 70 0d 00 	mov    %rax,0xd70(%rsp)
   9de17:	00 
   9de18:	48 8b 82 2f 02 00 00 	mov    0x22f(%rdx),%rax
   9de1f:	48 89 8c 24 78 0d 00 	mov    %rcx,0xd78(%rsp)
   9de26:	00 
   9de27:	48 8b 8a 47 02 00 00 	mov    0x247(%rdx),%rcx
   9de2e:	0f 10 82 1f 02 00 00 	movups 0x21f(%rdx),%xmm0
   9de35:	0f 29 84 24 80 0d 00 	movaps %xmm0,0xd80(%rsp)
   9de3c:	00 
   9de3d:	48 89 84 24 90 0d 00 	mov    %rax,0xd90(%rsp)
   9de44:	00 
   9de45:	48 8b 82 7f 02 00 00 	mov    0x27f(%rdx),%rax
   9de4c:	0f 10 82 37 02 00 00 	movups 0x237(%rdx),%xmm0
   9de53:	0f 11 84 24 98 0d 00 	movups %xmm0,0xd98(%rsp)
   9de5a:	00 
   9de5b:	0f 10 82 4f 02 00 00 	movups 0x24f(%rdx),%xmm0
   9de62:	48 89 8c 24 a8 0d 00 	mov    %rcx,0xda8(%rsp)
   9de69:	00 
   9de6a:	0f 29 84 24 b0 0d 00 	movaps %xmm0,0xdb0(%rsp)
   9de71:	00 
   9de72:	0f 10 82 5f 02 00 00 	movups 0x25f(%rdx),%xmm0
   9de79:	0f 29 84 24 c0 0d 00 	movaps %xmm0,0xdc0(%rsp)
   9de80:	00 
   9de81:	0f 10 82 6f 02 00 00 	movups 0x26f(%rdx),%xmm0
   9de88:	0f 29 84 24 d0 0d 00 	movaps %xmm0,0xdd0(%rsp)
   9de8f:	00 
   9de90:	0f 10 82 87 02 00 00 	movups 0x287(%rdx),%xmm0
   9de97:	48 89 84 24 e0 0d 00 	mov    %rax,0xde0(%rsp)
   9de9e:	00 
   9de9f:	0f 11 84 24 e8 0d 00 	movups %xmm0,0xde8(%rsp)
   9dea6:	00 
   9dea7:	0f 10 82 97 02 00 00 	movups 0x297(%rdx),%xmm0
   9deae:	0f 11 84 24 f8 0d 00 	movups %xmm0,0xdf8(%rsp)
   9deb5:	00 
   9deb6:	0f 10 82 a7 02 00 00 	movups 0x2a7(%rdx),%xmm0
   9debd:	0f 11 84 24 08 0e 00 	movups %xmm0,0xe08(%rsp)
   9dec4:	00 
   9dec5:	0f 10 82 b7 02 00 00 	movups 0x2b7(%rdx),%xmm0
   9decc:	0f 11 84 24 18 0e 00 	movups %xmm0,0xe18(%rsp)
   9ded3:	00 
   9ded4:	0f 10 82 c7 02 00 00 	movups 0x2c7(%rdx),%xmm0
   9dedb:	0f 11 84 24 28 0e 00 	movups %xmm0,0xe28(%rsp)
   9dee2:	00 
   9dee3:	0f 10 82 d7 02 00 00 	movups 0x2d7(%rdx),%xmm0
   9deea:	0f 11 84 24 38 0e 00 	movups %xmm0,0xe38(%rsp)
   9def1:	00 
   9def2:	0f 10 82 e7 02 00 00 	movups 0x2e7(%rdx),%xmm0
   9def9:	0f 11 84 24 48 0e 00 	movups %xmm0,0xe48(%rsp)
   9df00:	00 
   9df01:	0f 10 82 f7 02 00 00 	movups 0x2f7(%rdx),%xmm0
   9df08:	0f 11 84 24 58 0e 00 	movups %xmm0,0xe58(%rsp)
   9df0f:	00 
   9df10:	0f 10 82 07 03 00 00 	movups 0x307(%rdx),%xmm0
   9df17:	48 8b 82 17 03 00 00 	mov    0x317(%rdx),%rax
   9df1e:	48 8b 8a 3f 03 00 00 	mov    0x33f(%rdx),%rcx
   9df25:	0f 11 84 24 68 0e 00 	movups %xmm0,0xe68(%rsp)
   9df2c:	00 
   9df2d:	48 89 84 24 78 0e 00 	mov    %rax,0xe78(%rsp)
   9df34:	00 
   9df35:	48 8b 82 57 03 00 00 	mov    0x357(%rdx),%rax
   9df3c:	0f 10 82 1f 03 00 00 	movups 0x31f(%rdx),%xmm0
   9df43:	0f 29 84 24 80 0e 00 	movaps %xmm0,0xe80(%rsp)
   9df4a:	00 
   9df4b:	0f 10 82 2f 03 00 00 	movups 0x32f(%rdx),%xmm0
   9df52:	0f 29 84 24 90 0e 00 	movaps %xmm0,0xe90(%rsp)
   9df59:	00 
   9df5a:	48 89 8c 24 a0 0e 00 	mov    %rcx,0xea0(%rsp)
   9df61:	00 
   9df62:	0f b6 8a 5f 03 00 00 	movzbl 0x35f(%rdx),%ecx
   9df69:	0f 10 82 47 03 00 00 	movups 0x347(%rdx),%xmm0
   9df70:	0f 11 84 24 a8 0e 00 	movups %xmm0,0xea8(%rsp)
   9df77:	00 
   9df78:	48 89 84 24 b8 0e 00 	mov    %rax,0xeb8(%rsp)
   9df7f:	00 
   9df80:	48 8b 82 90 03 00 00 	mov    0x390(%rdx),%rax
   9df87:	48 89 8c 24 c0 0e 00 	mov    %rcx,0xec0(%rsp)
   9df8e:	00 
   9df8f:	0f b6 8a a8 03 00 00 	movzbl 0x3a8(%rdx),%ecx
   9df96:	0f 10 82 60 03 00 00 	movups 0x360(%rdx),%xmm0
   9df9d:	0f 11 84 24 c8 0e 00 	movups %xmm0,0xec8(%rsp)
   9dfa4:	00 
   9dfa5:	0f 10 82 70 03 00 00 	movups 0x370(%rdx),%xmm0
   9dfac:	0f 11 84 24 d8 0e 00 	movups %xmm0,0xed8(%rsp)
   9dfb3:	00 
   9dfb4:	0f 10 82 80 03 00 00 	movups 0x380(%rdx),%xmm0
   9dfbb:	0f 11 84 24 e8 0e 00 	movups %xmm0,0xee8(%rsp)
   9dfc2:	00 
   9dfc3:	48 89 84 24 f8 0e 00 	mov    %rax,0xef8(%rsp)
   9dfca:	00 
   9dfcb:	48 8b 82 b9 03 00 00 	mov    0x3b9(%rdx),%rax
   9dfd2:	0f 10 82 98 03 00 00 	movups 0x398(%rdx),%xmm0
   9dfd9:	0f 29 84 24 00 0f 00 	movaps %xmm0,0xf00(%rsp)
   9dfe0:	00 
   9dfe1:	48 89 8c 24 10 0f 00 	mov    %rcx,0xf10(%rsp)
   9dfe8:	00 
   9dfe9:	48 8b 8a 01 04 00 00 	mov    0x401(%rdx),%rcx
   9dff0:	0f 10 82 a9 03 00 00 	movups 0x3a9(%rdx),%xmm0
   9dff7:	0f 11 84 24 18 0f 00 	movups %xmm0,0xf18(%rsp)
   9dffe:	00 
   9dfff:	0f 10 82 c1 03 00 00 	movups 0x3c1(%rdx),%xmm0
   9e006:	48 89 84 24 28 0f 00 	mov    %rax,0xf28(%rsp)
   9e00d:	00 
   9e00e:	0f 29 84 24 30 0f 00 	movaps %xmm0,0xf30(%rsp)
   9e015:	00 
   9e016:	0f 10 82 d1 03 00 00 	movups 0x3d1(%rdx),%xmm0
   9e01d:	0f 29 84 24 40 0f 00 	movaps %xmm0,0xf40(%rsp)
   9e024:	00 
   9e025:	0f 10 82 e1 03 00 00 	movups 0x3e1(%rdx),%xmm0
   9e02c:	0f 29 84 24 50 0f 00 	movaps %xmm0,0xf50(%rsp)
   9e033:	00 
   9e034:	0f 10 82 f1 03 00 00 	movups 0x3f1(%rdx),%xmm0
   9e03b:	0f 29 84 24 60 0f 00 	movaps %xmm0,0xf60(%rsp)
   9e042:	00 
   9e043:	0f 10 82 09 04 00 00 	movups 0x409(%rdx),%xmm0
   9e04a:	48 89 8c 24 70 0f 00 	mov    %rcx,0xf70(%rsp)
   9e051:	00 
   9e052:	0f 11 84 24 78 0f 00 	movups %xmm0,0xf78(%rsp)
   9e059:	00 
   9e05a:	0f 10 82 19 04 00 00 	movups 0x419(%rdx),%xmm0
   9e061:	0f b6 82 69 04 00 00 	movzbl 0x469(%rdx),%eax
   9e068:	0f b7 8a 6a 04 00 00 	movzwl 0x46a(%rdx),%ecx
   9e06f:	c1 e1 08             	shl    $0x8,%ecx
   9e072:	48 09 c1             	or     %rax,%rcx
   9e075:	48 8b 82 49 04 00 00 	mov    0x449(%rdx),%rax
   9e07c:	0f 11 84 24 88 0f 00 	movups %xmm0,0xf88(%rsp)
   9e083:	00 
   9e084:	0f 10 82 29 04 00 00 	movups 0x429(%rdx),%xmm0
   9e08b:	0f 11 84 24 98 0f 00 	movups %xmm0,0xf98(%rsp)
   9e092:	00 
   9e093:	0f 10 82 39 04 00 00 	movups 0x439(%rdx),%xmm0
   9e09a:	0f 11 84 24 a8 0f 00 	movups %xmm0,0xfa8(%rsp)
   9e0a1:	00 
   9e0a2:	48 89 84 24 b8 0f 00 	mov    %rax,0xfb8(%rsp)
   9e0a9:	00 
   9e0aa:	48 8b 82 61 04 00 00 	mov    0x461(%rdx),%rax
   9e0b1:	0f 10 82 51 04 00 00 	movups 0x451(%rdx),%xmm0
   9e0b8:	0f 29 84 24 c0 0f 00 	movaps %xmm0,0xfc0(%rsp)
   9e0bf:	00 
   9e0c0:	0f 10 82 6c 04 00 00 	movups 0x46c(%rdx),%xmm0
   9e0c7:	48 89 84 24 d0 0f 00 	mov    %rax,0xfd0(%rsp)
   9e0ce:	00 
   9e0cf:	48 89 8c 24 d8 0f 00 	mov    %rcx,0xfd8(%rsp)
   9e0d6:	00 
   9e0d7:	0f 29 84 24 e0 0f 00 	movaps %xmm0,0xfe0(%rsp)
   9e0de:	00 
   9e0df:	0f 10 82 7c 04 00 00 	movups 0x47c(%rdx),%xmm0
   9e0e6:	0f b7 82 8c 04 00 00 	movzwl 0x48c(%rdx),%eax
   9e0ed:	0f 29 84 24 f0 0f 00 	movaps %xmm0,0xff0(%rsp)
   9e0f4:	00 
   9e0f5:	0f 10 82 8e 04 00 00 	movups 0x48e(%rdx),%xmm0
   9e0fc:	48 89 84 24 00 10 00 	mov    %rax,0x1000(%rsp)
   9e103:	00 
   9e104:	0f 11 84 24 08 10 00 	movups %xmm0,0x1008(%rsp)
   9e10b:	00 
   9e10c:	0f 10 82 9e 04 00 00 	movups 0x49e(%rdx),%xmm0
   9e113:	0f b6 82 ae 04 00 00 	movzbl 0x4ae(%rdx),%eax
   9e11a:	0f 11 84 24 18 10 00 	movups %xmm0,0x1018(%rsp)
   9e121:	00 
   9e122:	0f 10 82 af 04 00 00 	movups 0x4af(%rdx),%xmm0
   9e129:	88 84 24 28 10 00 00 	mov    %al,0x1028(%rsp)
   9e130:	0f 29 84 24 30 10 00 	movaps %xmm0,0x1030(%rsp)
   9e137:	00 
   9e138:	0f 10 82 bf 04 00 00 	movups 0x4bf(%rdx),%xmm0
   9e13f:	0f 29 84 24 40 10 00 	movaps %xmm0,0x1040(%rsp)
   9e146:	00 
   9e147:	f3 0f 6f 82 cf 04 00 	movdqu 0x4cf(%rdx),%xmm0
   9e14e:	00 
   9e14f:	66 0f 7f 84 24 50 10 	movdqa %xmm0,0x1050(%rsp)
   9e156:	00 00 
   9e158:	4c 8b 3a             	mov    (%rdx),%r15
   9e15b:	48 8b 82 df 04 00 00 	mov    0x4df(%rdx),%rax
   9e162:	48 89 84 24 60 10 00 	mov    %rax,0x1060(%rsp)
   9e169:	00 
   9e16a:	48 8b 84 24 d0 00 00 	mov    0xd0(%rsp),%rax
   9e171:	00 
   9e172:	48 83 f8 04          	cmp    $0x4,%rax
   9e176:	0f 87 a8 e7 ff ff    	ja     9c924 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x3324>
   9e17c:	b9 16 00 00 00       	mov    $0x16,%ecx
   9e181:	48 0f a3 c1          	bt     %rax,%rcx
   9e185:	0f 83 99 e7 ff ff    	jae    9c924 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x3324>
   9e18b:	48 8b 8c 24 c0 00 00 	mov    0xc0(%rsp),%rcx
   9e192:	00 
   9e193:	48 85 c9             	test   %rcx,%rcx
   9e196:	0f 88 1a 0b 00 00    	js     9ecb6 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x56b6>
   9e19c:	89 cb                	mov    %ecx,%ebx
   9e19e:	49 3b 5f 08          	cmp    0x8(%r15),%rbx
   9e1a2:	0f 8d 0e 0b 00 00    	jge    9ecb6 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x56b6>
   9e1a8:	49 8b 17             	mov    (%r15),%rdx
   9e1ab:	4c 69 e3 60 01 00 00 	imul   $0x160,%rbx,%r12
   9e1b2:	4a 83 3c 22 06       	cmpq   $0x6,(%rdx,%r12,1)
   9e1b7:	0f 85 f9 0a 00 00    	jne    9ecb6 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x56b6>
   9e1bd:	48 c1 e9 20          	shr    $0x20,%rcx
   9e1c1:	4a 39 4c 22 19       	cmp    %rcx,0x19(%rdx,%r12,1)
   9e1c6:	0f 85 ea 0a 00 00    	jne    9ecb6 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x56b6>
   9e1cc:	48 83 f8 01          	cmp    $0x1,%rax
   9e1d0:	0f 84 f7 00 00 00    	je     9e2cd <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4ccd>
   9e1d6:	48 83 f8 04          	cmp    $0x4,%rax
   9e1da:	0f 85 0b 01 00 00    	jne    9e2eb <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4ceb>
   9e1e0:	48 8d bc 24 40 0b 00 	lea    0xb40(%rsp),%rdi
   9e1e7:	00 
   9e1e8:	48 8d b4 24 d0 00 00 	lea    0xd0(%rsp),%rsi
   9e1ef:	00 
   9e1f0:	e8 db ae ff ff       	call   990d0 <_Wfl.fl_obj_index$6801bb8d311b3e64>
   9e1f5:	49 3b 47 08          	cmp    0x8(%r15),%rax
   9e1f9:	0f 83 cf 0c 00 00    	jae    9eece <_Wfl.fl_vm_exec$a5081fe92876eb37+0x58ce>
   9e1ff:	49 8b 0f             	mov    (%r15),%rcx
   9e202:	48 69 c0 60 01 00 00 	imul   $0x160,%rax,%rax
   9e209:	48 8b 4c 01 11       	mov    0x11(%rcx,%rax,1),%rcx
   9e20e:	e9 59 01 00 00       	jmp    9e36c <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4d6c>
   9e213:	49 83 fd 04          	cmp    $0x4,%r13
   9e217:	0f 87 a3 02 00 00    	ja     9e4c0 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4ec0>
   9e21d:	b8 16 00 00 00       	mov    $0x16,%eax
   9e222:	4c 0f a3 e8          	bt     %r13,%rax
   9e226:	0f 83 94 02 00 00    	jae    9e4c0 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4ec0>
   9e22c:	48 8b 44 24 68       	mov    0x68(%rsp),%rax
   9e231:	48 83 c0 f4          	add    $0xfffffffffffffff4,%rax
   9e235:	48 83 f8 f8          	cmp    $0xfffffffffffffff8,%rax
   9e239:	0f 82 95 0b 00 00    	jb     9edd4 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x57d4>
   9e23f:	4d 85 e4             	test   %r12,%r12
   9e242:	0f 88 8c 0b 00 00    	js     9edd4 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x57d4>
   9e248:	44 89 e3             	mov    %r12d,%ebx
   9e24b:	48 3b 5d 08          	cmp    0x8(%rbp),%rbx
   9e24f:	0f 8d 7f 0b 00 00    	jge    9edd4 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x57d4>
   9e255:	48 8b 45 00          	mov    0x0(%rbp),%rax
   9e259:	4c 69 fb 60 01 00 00 	imul   $0x160,%rbx,%r15
   9e260:	48 8b 4c 24 68       	mov    0x68(%rsp),%rcx
   9e265:	4a 39 0c 38          	cmp    %rcx,(%rax,%r15,1)
   9e269:	0f 85 65 0b 00 00    	jne    9edd4 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x57d4>
   9e26f:	49 c1 ec 20          	shr    $0x20,%r12
   9e273:	4e 39 64 38 19       	cmp    %r12,0x19(%rax,%r15,1)
   9e278:	0f 85 56 0b 00 00    	jne    9edd4 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x57d4>
   9e27e:	49 83 fd 04          	cmp    $0x4,%r13
   9e282:	0f 85 77 01 00 00    	jne    9e3ff <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4dff>
   9e288:	48 8d bc 24 10 06 00 	lea    0x610(%rsp),%rdi
   9e28f:	00 
   9e290:	48 8d b4 24 88 00 00 	lea    0x88(%rsp),%rsi
   9e297:	00 
   9e298:	e8 33 ae ff ff       	call   990d0 <_Wfl.fl_obj_index$6801bb8d311b3e64>
   9e29d:	48 3b 45 08          	cmp    0x8(%rbp),%rax
   9e2a1:	0f 83 46 0a 00 00    	jae    9eced <_Wfl.fl_vm_exec$a5081fe92876eb37+0x56ed>
   9e2a7:	48 8b 4d 00          	mov    0x0(%rbp),%rcx
   9e2ab:	48 69 c0 60 01 00 00 	imul   $0x160,%rax,%rax
   9e2b2:	48 8b 4c 01 11       	mov    0x11(%rcx,%rax,1),%rcx
   9e2b7:	e9 e4 01 00 00       	jmp    9e4a0 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4ea0>
   9e2bc:	bb 0f 00 00 00       	mov    $0xf,%ebx
   9e2c1:	4c 8d 2d db 64 f7 ff 	lea    -0x89b25(%rip),%r13        # 147a3 <__abi_tag+0x144a7>
   9e2c8:	e9 8e f5 ff ff       	jmp    9d85b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x425b>
   9e2cd:	b8 6b ca eb 85       	mov    $0x85ebca6b,%eax
   9e2d2:	48 8d 88 4e af 4b 18 	lea    0x184baf4e(%rax),%rcx
   9e2d9:	48 83 bc 24 d8 00 00 	cmpq   $0x0,0xd8(%rsp)
   9e2e0:	00 00 
   9e2e2:	48 0f 44 c8          	cmove  %rax,%rcx
   9e2e6:	e9 81 00 00 00       	jmp    9e36c <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4d6c>
   9e2eb:	48 8b 84 24 d8 00 00 	mov    0xd8(%rsp),%rax
   9e2f2:	00 
   9e2f3:	0f b6 c8             	movzbl %al,%ecx
   9e2f6:	81 f1 c5 9d 1c 81    	xor    $0x811c9dc5,%ecx
   9e2fc:	69 c9 93 01 00 01    	imul   $0x1000193,%ecx,%ecx
   9e302:	0f b6 d4             	movzbl %ah,%edx
   9e305:	31 ca                	xor    %ecx,%edx
   9e307:	69 ca 93 01 00 01    	imul   $0x1000193,%edx,%ecx
   9e30d:	89 c2                	mov    %eax,%edx
   9e30f:	c1 ea 10             	shr    $0x10,%edx
   9e312:	0f b6 d2             	movzbl %dl,%edx
   9e315:	31 ca                	xor    %ecx,%edx
   9e317:	69 ca 93 01 00 01    	imul   $0x1000193,%edx,%ecx
   9e31d:	89 c2                	mov    %eax,%edx
   9e31f:	c1 ea 18             	shr    $0x18,%edx
   9e322:	31 ca                	xor    %ecx,%edx
   9e324:	69 ca 93 01 00 01    	imul   $0x1000193,%edx,%ecx
   9e32a:	48 89 c2             	mov    %rax,%rdx
   9e32d:	48 c1 ea 20          	shr    $0x20,%rdx
   9e331:	0f b6 d2             	movzbl %dl,%edx
   9e334:	31 ca                	xor    %ecx,%edx
   9e336:	69 ca 93 01 00 01    	imul   $0x1000193,%edx,%ecx
   9e33c:	48 89 c2             	mov    %rax,%rdx
   9e33f:	48 c1 ea 28          	shr    $0x28,%rdx
   9e343:	0f b6 d2             	movzbl %dl,%edx
   9e346:	31 ca                	xor    %ecx,%edx
   9e348:	69 ca 93 01 00 01    	imul   $0x1000193,%edx,%ecx
   9e34e:	48 89 c2             	mov    %rax,%rdx
   9e351:	48 c1 ea 30          	shr    $0x30,%rdx
   9e355:	0f b6 d2             	movzbl %dl,%edx
   9e358:	31 ca                	xor    %ecx,%edx
   9e35a:	69 ca 93 01 00 01    	imul   $0x1000193,%edx,%ecx
   9e360:	48 c1 e8 38          	shr    $0x38,%rax
   9e364:	31 c8                	xor    %ecx,%eax
   9e366:	69 c8 93 01 00 01    	imul   $0x1000193,%eax,%ecx
   9e36c:	48 8d bc 24 40 0b 00 	lea    0xb40(%rsp),%rdi
   9e373:	00 
   9e374:	48 89 de             	mov    %rbx,%rsi
   9e377:	48 8d 94 24 d0 00 00 	lea    0xd0(%rsp),%rdx
   9e37e:	00 
   9e37f:	e8 4c e4 07 00       	call   11c7d0 <_Wfl.fl_map_probe$234ad4c981fd6647>
   9e384:	84 d2                	test   %dl,%dl
   9e386:	0f 84 98 e5 ff ff    	je     9c924 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x3324>
   9e38c:	49 3b 5f 08          	cmp    0x8(%r15),%rbx
   9e390:	0f 83 38 0b 00 00    	jae    9eece <_Wfl.fl_vm_exec$a5081fe92876eb37+0x58ce>
   9e396:	49 8b 0f             	mov    (%r15),%rcx
   9e399:	4a 8b 54 21 39       	mov    0x39(%rcx,%r12,1),%rdx
   9e39e:	48 3b 42 08          	cmp    0x8(%rdx),%rax
   9e3a2:	0f 83 26 0b 00 00    	jae    9eece <_Wfl.fl_vm_exec$a5081fe92876eb37+0x58ce>
   9e3a8:	48 8b 12             	mov    (%rdx),%rdx
   9e3ab:	48 8b 04 c2          	mov    (%rdx,%rax,8),%rax
   9e3af:	48 ff c8             	dec    %rax
   9e3b2:	0f 80 b1 08 00 00    	jo     9ec69 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5669>
   9e3b8:	4a 8b 4c 21 31       	mov    0x31(%rcx,%r12,1),%rcx
   9e3bd:	48 3b 41 08          	cmp    0x8(%rcx),%rax
   9e3c1:	0f 83 07 0b 00 00    	jae    9eece <_Wfl.fl_vm_exec$a5081fe92876eb37+0x58ce>
   9e3c7:	48 8b 09             	mov    (%rcx),%rcx
   9e3ca:	48 c1 e0 06          	shl    $0x6,%rax
   9e3ce:	48 8b 54 01 28       	mov    0x28(%rcx,%rax,1),%rdx
   9e3d3:	f3 0f 6f 44 01 18    	movdqu 0x18(%rcx,%rax,1),%xmm0
   9e3d9:	66 0f 7f 84 24 30 01 	movdqa %xmm0,0x130(%rsp)
   9e3e0:	00 00 
   9e3e2:	48 89 94 24 40 01 00 	mov    %rdx,0x140(%rsp)
   9e3e9:	00 
   9e3ea:	4c 8b 74 24 10       	mov    0x10(%rsp),%r14
   9e3ef:	4c 89 f7             	mov    %r14,%rdi
   9e3f2:	48 8d b4 24 30 01 00 	lea    0x130(%rsp),%rsi
   9e3f9:	00 
   9e3fa:	e9 cd c0 ff ff       	jmp    9a4cc <_Wfl.fl_vm_exec$a5081fe92876eb37+0xecc>
   9e3ff:	49 83 fd 01          	cmp    $0x1,%r13
   9e403:	75 1b                	jne    9e420 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4e20>
   9e405:	b8 6b ca eb 85       	mov    $0x85ebca6b,%eax
   9e40a:	48 8d 88 4e af 4b 18 	lea    0x184baf4e(%rax),%rcx
   9e411:	48 83 7c 24 38 00    	cmpq   $0x0,0x38(%rsp)
   9e417:	48 0f 44 c8          	cmove  %rax,%rcx
   9e41b:	e9 80 00 00 00       	jmp    9e4a0 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4ea0>
   9e420:	48 8b 54 24 38       	mov    0x38(%rsp),%rdx
   9e425:	0f b6 c2             	movzbl %dl,%eax
   9e428:	35 c5 9d 1c 81       	xor    $0x811c9dc5,%eax
   9e42d:	69 c0 93 01 00 01    	imul   $0x1000193,%eax,%eax
   9e433:	0f b6 ce             	movzbl %dh,%ecx
   9e436:	31 c1                	xor    %eax,%ecx
   9e438:	69 c1 93 01 00 01    	imul   $0x1000193,%ecx,%eax
   9e43e:	89 d1                	mov    %edx,%ecx
   9e440:	c1 e9 10             	shr    $0x10,%ecx
   9e443:	0f b6 c9             	movzbl %cl,%ecx
   9e446:	31 c1                	xor    %eax,%ecx
   9e448:	69 c1 93 01 00 01    	imul   $0x1000193,%ecx,%eax
   9e44e:	89 d1                	mov    %edx,%ecx
   9e450:	c1 e9 18             	shr    $0x18,%ecx
   9e453:	31 c1                	xor    %eax,%ecx
   9e455:	69 c1 93 01 00 01    	imul   $0x1000193,%ecx,%eax
   9e45b:	48 89 d1             	mov    %rdx,%rcx
   9e45e:	48 c1 e9 20          	shr    $0x20,%rcx
   9e462:	0f b6 c9             	movzbl %cl,%ecx
   9e465:	31 c1                	xor    %eax,%ecx
   9e467:	69 c1 93 01 00 01    	imul   $0x1000193,%ecx,%eax
   9e46d:	48 89 d1             	mov    %rdx,%rcx
   9e470:	48 c1 e9 28          	shr    $0x28,%rcx
   9e474:	0f b6 c9             	movzbl %cl,%ecx
   9e477:	31 c1                	xor    %eax,%ecx
   9e479:	69 c1 93 01 00 01    	imul   $0x1000193,%ecx,%eax
   9e47f:	48 89 d1             	mov    %rdx,%rcx
   9e482:	48 c1 e9 30          	shr    $0x30,%rcx
   9e486:	0f b6 c9             	movzbl %cl,%ecx
   9e489:	31 c1                	xor    %eax,%ecx
   9e48b:	69 c1 93 01 00 01    	imul   $0x1000193,%ecx,%eax
   9e491:	48 89 d1             	mov    %rdx,%rcx
   9e494:	48 c1 e9 38          	shr    $0x38,%rcx
   9e498:	31 c1                	xor    %eax,%ecx
   9e49a:	69 c9 93 01 00 01    	imul   $0x1000193,%ecx,%ecx
   9e4a0:	48 8d bc 24 10 06 00 	lea    0x610(%rsp),%rdi
   9e4a7:	00 
   9e4a8:	48 89 de             	mov    %rbx,%rsi
   9e4ab:	48 8d 94 24 88 00 00 	lea    0x88(%rsp),%rdx
   9e4b2:	00 
   9e4b3:	e8 18 e3 07 00       	call   11c7d0 <_Wfl.fl_map_probe$234ad4c981fd6647>
   9e4b8:	84 d2                	test   %dl,%dl
   9e4ba:	0f 85 e2 f4 ff ff    	jne    9d9a2 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x43a2>
   9e4c0:	48 8d bc 24 10 06 00 	lea    0x610(%rsp),%rdi
   9e4c7:	00 
   9e4c8:	48 8d b4 24 50 01 00 	lea    0x150(%rsp),%rsi
   9e4cf:	00 
   9e4d0:	48 8b 54 24 38       	mov    0x38(%rsp),%rdx
   9e4d5:	e8 e6 01 01 00       	call   ae6c0 <_Wfl.fl_missing_global$adcb0e841291d8c3>
   9e4da:	ba 04 00 00 00       	mov    $0x4,%edx
   9e4df:	48 8b 7c 24 10       	mov    0x10(%rsp),%rdi
   9e4e4:	48 8d 35 cd 0b f8 ff 	lea    -0x7f433(%rip),%rsi        # 1f0b8 <__abi_tag+0x1edbc>
   9e4eb:	48 89 c1             	mov    %rax,%rcx
   9e4ee:	4c 8d 84 24 f0 21 00 	lea    0x21f0(%rsp),%r8
   9e4f5:	00 
   9e4f6:	e8 45 0e 03 00       	call   cf340 <_Wfl.fl_raise_bytes$05a094d3779625bf>
   9e4fb:	f3 0f 6f 44 24 58    	movdqu 0x58(%rsp),%xmm0
   9e501:	48 8d 44 24 50       	lea    0x50(%rsp),%rax
   9e506:	48 8b 00             	mov    (%rax),%rax
   9e509:	48 89 84 24 68 05 00 	mov    %rax,0x568(%rsp)
   9e510:	00 
   9e511:	f3 0f 7f 84 24 70 05 	movdqu %xmm0,0x570(%rsp)
   9e518:	00 00 
   9e51a:	48 8b 44 24 10       	mov    0x10(%rsp),%rax
   9e51f:	48 05 5c 01 00 00    	add    $0x15c,%rax
   9e525:	80 38 00             	cmpb   $0x0,(%rax)
   9e528:	74 14                	je     9e53e <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4f3e>
   9e52a:	48 8b 6c 24 18       	mov    0x18(%rsp),%rbp
   9e52f:	48 8b 5c 24 08       	mov    0x8(%rsp),%rbx
   9e534:	4c 8b 74 24 10       	mov    0x10(%rsp),%r14
   9e539:	e9 e7 ca ff ff       	jmp    9b025 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x1a25>
   9e53e:	4c 8b 74 24 10       	mov    0x10(%rsp),%r14
   9e543:	4c 89 f7             	mov    %r14,%rdi
   9e546:	48 8d b4 24 68 05 00 	lea    0x568(%rsp),%rsi
   9e54d:	00 
   9e54e:	e9 79 bf ff ff       	jmp    9a4cc <_Wfl.fl_vm_exec$a5081fe92876eb37+0xecc>
   9e553:	48 8d 44 24 50       	lea    0x50(%rsp),%rax
   9e558:	f3 0f 6f 44 24 58    	movdqu 0x58(%rsp),%xmm0
   9e55e:	eb 28                	jmp    9e588 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4f88>
   9e560:	49 89 96 b5 01 00 00 	mov    %rdx,0x1b5(%r14)
   9e567:	4c 89 f7             	mov    %r14,%rdi
   9e56a:	e8 71 d5 02 00       	call   cbae0 <_Wfl.fl_trace_attach$26a4ad1c7c1e1b81>
   9e56f:	eb 07                	jmp    9e578 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4f78>
   9e571:	49 89 96 b5 01 00 00 	mov    %rdx,0x1b5(%r14)
   9e578:	41 0f 10 86 65 01 00 	movups 0x165(%r14),%xmm0
   9e57f:	00 
   9e580:	48 8b 84 24 a0 00 00 	mov    0xa0(%rsp),%rax
   9e587:	00 
   9e588:	48 8b 00             	mov    (%rax),%rax
   9e58b:	48 8b 8c 24 08 01 00 	mov    0x108(%rsp),%rcx
   9e592:	00 
   9e593:	48 89 01             	mov    %rax,(%rcx)
   9e596:	f3 0f 7f 41 08       	movdqu %xmm0,0x8(%rcx)
   9e59b:	48 81 c4 78 22 00 00 	add    $0x2278,%rsp
   9e5a2:	5b                   	pop    %rbx
   9e5a3:	41 5c                	pop    %r12
   9e5a5:	41 5d                	pop    %r13
   9e5a7:	41 5e                	pop    %r14
   9e5a9:	41 5f                	pop    %r15
   9e5ab:	5d                   	pop    %rbp
   9e5ac:	c3                   	ret
   9e5ad:	48 8d 35 3e 62 f7 ff 	lea    -0x89dc2(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9e5b4:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9e5b9:	b9 68 02 00 00       	mov    $0x268,%ecx
   9e5be:	41 b8 15 00 00 00    	mov    $0x15,%r8d
   9e5c4:	bf 03 00 00 00       	mov    $0x3,%edi
   9e5c9:	e8 02 84 08 00       	call   1269d0 <__wolf_rt_trap_at>
   9e5ce:	48 8d 35 1d 62 f7 ff 	lea    -0x89de3(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9e5d5:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9e5da:	b9 f9 01 00 00       	mov    $0x1f9,%ecx
   9e5df:	eb 4d                	jmp    9e62e <_Wfl.fl_vm_exec$a5081fe92876eb37+0x502e>
   9e5e1:	48 8d 35 0a 62 f7 ff 	lea    -0x89df6(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9e5e8:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9e5ed:	b9 9e 03 00 00       	mov    $0x39e,%ecx
   9e5f2:	eb 3a                	jmp    9e62e <_Wfl.fl_vm_exec$a5081fe92876eb37+0x502e>
   9e5f4:	48 8d 35 f7 61 f7 ff 	lea    -0x89e09(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9e5fb:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9e600:	b9 b5 03 00 00       	mov    $0x3b5,%ecx
   9e605:	eb 27                	jmp    9e62e <_Wfl.fl_vm_exec$a5081fe92876eb37+0x502e>
   9e607:	48 8d 35 e4 61 f7 ff 	lea    -0x89e1c(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9e60e:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9e613:	b9 1a 02 00 00       	mov    $0x21a,%ecx
   9e618:	e9 e4 00 00 00       	jmp    9e701 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5101>
   9e61d:	48 8d 35 ce 61 f7 ff 	lea    -0x89e32(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9e624:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9e629:	b9 fd 01 00 00       	mov    $0x1fd,%ecx
   9e62e:	41 b8 0d 00 00 00    	mov    $0xd,%r8d
   9e634:	bf 01 00 00 00       	mov    $0x1,%edi
   9e639:	e8 92 83 08 00       	call   1269d0 <__wolf_rt_trap_at>
   9e63e:	48 8d 35 ad 61 f7 ff 	lea    -0x89e53(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9e645:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9e64a:	b9 1f 02 00 00       	mov    $0x21f,%ecx
   9e64f:	e9 ad 00 00 00       	jmp    9e701 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5101>
   9e654:	48 8d 35 97 61 f7 ff 	lea    -0x89e69(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9e65b:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9e660:	b9 0f 02 00 00       	mov    $0x20f,%ecx
   9e665:	41 b8 11 00 00 00    	mov    $0x11,%r8d
   9e66b:	bf 01 00 00 00       	mov    $0x1,%edi
   9e670:	e8 5b 83 08 00       	call   1269d0 <__wolf_rt_trap_at>
   9e675:	48 8d 35 76 61 f7 ff 	lea    -0x89e8a(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9e67c:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9e681:	b9 24 02 00 00       	mov    $0x224,%ecx
   9e686:	eb 79                	jmp    9e701 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5101>
   9e688:	48 8d 35 63 61 f7 ff 	lea    -0x89e9d(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9e68f:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9e694:	b9 47 02 00 00       	mov    $0x247,%ecx
   9e699:	eb 66                	jmp    9e701 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5101>
   9e69b:	48 8d 35 50 61 f7 ff 	lea    -0x89eb0(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9e6a2:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9e6a7:	b9 44 02 00 00       	mov    $0x244,%ecx
   9e6ac:	41 b8 18 00 00 00    	mov    $0x18,%r8d
   9e6b2:	bf 01 00 00 00       	mov    $0x1,%edi
   9e6b7:	e8 14 83 08 00       	call   1269d0 <__wolf_rt_trap_at>
   9e6bc:	48 8d 35 2f 61 f7 ff 	lea    -0x89ed1(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9e6c3:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9e6c8:	b9 4d 02 00 00       	mov    $0x24d,%ecx
   9e6cd:	eb 32                	jmp    9e701 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5101>
   9e6cf:	48 8d 35 1c 61 f7 ff 	lea    -0x89ee4(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9e6d6:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9e6db:	b9 43 02 00 00       	mov    $0x243,%ecx
   9e6e0:	41 b8 3d 00 00 00    	mov    $0x3d,%r8d
   9e6e6:	bf 01 00 00 00       	mov    $0x1,%edi
   9e6eb:	e8 e0 82 08 00       	call   1269d0 <__wolf_rt_trap_at>
   9e6f0:	48 8d 35 fb 60 f7 ff 	lea    -0x89f05(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9e6f7:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9e6fc:	b9 62 02 00 00       	mov    $0x262,%ecx
   9e701:	41 b8 15 00 00 00    	mov    $0x15,%r8d
   9e707:	bf 01 00 00 00       	mov    $0x1,%edi
   9e70c:	e8 bf 82 08 00       	call   1269d0 <__wolf_rt_trap_at>
   9e711:	0f 10 84 24 70 01 00 	movups 0x170(%rsp),%xmm0
   9e718:	00 
   9e719:	48 8d 84 24 68 01 00 	lea    0x168(%rsp),%rax
   9e720:	00 
   9e721:	e9 62 fe ff ff       	jmp    9e588 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4f88>
   9e726:	48 8d 35 c5 60 f7 ff 	lea    -0x89f3b(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9e72d:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9e732:	b9 00 02 00 00       	mov    $0x200,%ecx
   9e737:	eb 11                	jmp    9e74a <_Wfl.fl_vm_exec$a5081fe92876eb37+0x514a>
   9e739:	48 8d 35 b2 60 f7 ff 	lea    -0x89f4e(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9e740:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9e745:	b9 02 02 00 00       	mov    $0x202,%ecx
   9e74a:	41 b8 11 00 00 00    	mov    $0x11,%r8d
   9e750:	bf 04 00 00 00       	mov    $0x4,%edi
   9e755:	e8 76 82 08 00       	call   1269d0 <__wolf_rt_trap_at>
   9e75a:	48 8d 35 91 60 f7 ff 	lea    -0x89f6f(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9e761:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9e766:	b9 1a 02 00 00       	mov    $0x21a,%ecx
   9e76b:	e9 4e fe ff ff       	jmp    9e5be <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4fbe>
   9e770:	48 8d 35 7b 60 f7 ff 	lea    -0x89f85(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9e777:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9e77c:	b9 9e 03 00 00       	mov    $0x39e,%ecx
   9e781:	e9 f5 00 00 00       	jmp    9e87b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x527b>
   9e786:	48 8d 35 65 60 f7 ff 	lea    -0x89f9b(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9e78d:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9e792:	b9 b5 03 00 00       	mov    $0x3b5,%ecx
   9e797:	e9 df 00 00 00       	jmp    9e87b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x527b>
   9e79c:	48 8d 35 4f 60 f7 ff 	lea    -0x89fb1(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9e7a3:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9e7a8:	b9 07 02 00 00       	mov    $0x207,%ecx
   9e7ad:	e9 c9 00 00 00       	jmp    9e87b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x527b>
   9e7b2:	48 8d 35 39 60 f7 ff 	lea    -0x89fc7(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9e7b9:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9e7be:	b9 94 02 00 00       	mov    $0x294,%ecx
   9e7c3:	e9 63 01 00 00       	jmp    9e92b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x532b>
   9e7c8:	48 8d 35 23 60 f7 ff 	lea    -0x89fdd(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9e7cf:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9e7d4:	b9 8b 02 00 00       	mov    $0x28b,%ecx
   9e7d9:	e9 23 ff ff ff       	jmp    9e701 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5101>
   9e7de:	48 8d 35 0d 60 f7 ff 	lea    -0x89ff3(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9e7e5:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9e7ea:	b9 9d 03 00 00       	mov    $0x39d,%ecx
   9e7ef:	41 b8 37 00 00 00    	mov    $0x37,%r8d
   9e7f5:	bf 03 00 00 00       	mov    $0x3,%edi
   9e7fa:	e8 d1 81 08 00       	call   1269d0 <__wolf_rt_trap_at>
   9e7ff:	48 8d 35 ec 5f f7 ff 	lea    -0x8a014(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9e806:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9e80b:	b9 d5 02 00 00       	mov    $0x2d5,%ecx
   9e810:	e9 16 01 00 00       	jmp    9e92b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x532b>
   9e815:	48 8d 35 d6 5f f7 ff 	lea    -0x8a02a(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9e81c:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9e821:	b9 1f 02 00 00       	mov    $0x21f,%ecx
   9e826:	e9 93 fd ff ff       	jmp    9e5be <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4fbe>
   9e82b:	48 8d 35 c0 5f f7 ff 	lea    -0x8a040(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9e832:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9e837:	b9 ff 01 00 00       	mov    $0x1ff,%ecx
   9e83c:	eb 5e                	jmp    9e89c <_Wfl.fl_vm_exec$a5081fe92876eb37+0x529c>
   9e83e:	48 8d 35 ad 5f f7 ff 	lea    -0x8a053(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9e845:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9e84a:	b9 df 02 00 00       	mov    $0x2df,%ecx
   9e84f:	e9 d7 00 00 00       	jmp    9e92b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x532b>
   9e854:	48 8d 35 97 5f f7 ff 	lea    -0x8a069(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9e85b:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9e860:	b9 24 02 00 00       	mov    $0x224,%ecx
   9e865:	e9 54 fd ff ff       	jmp    9e5be <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4fbe>
   9e86a:	48 8d 35 81 5f f7 ff 	lea    -0x8a07f(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9e871:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9e876:	b9 08 02 00 00       	mov    $0x208,%ecx
   9e87b:	41 b8 0d 00 00 00    	mov    $0xd,%r8d
   9e881:	bf 03 00 00 00       	mov    $0x3,%edi
   9e886:	e8 45 81 08 00       	call   1269d0 <__wolf_rt_trap_at>
   9e88b:	48 8d 35 60 5f f7 ff 	lea    -0x8a0a0(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9e892:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9e897:	b9 0f 02 00 00       	mov    $0x20f,%ecx
   9e89c:	41 b8 11 00 00 00    	mov    $0x11,%r8d
   9e8a2:	bf 03 00 00 00       	mov    $0x3,%edi
   9e8a7:	e8 24 81 08 00       	call   1269d0 <__wolf_rt_trap_at>
   9e8ac:	48 8d 35 3f 5f f7 ff 	lea    -0x8a0c1(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9e8b3:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9e8b8:	b9 a7 02 00 00       	mov    $0x2a7,%ecx
   9e8bd:	e9 3f fe ff ff       	jmp    9e701 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5101>
   9e8c2:	48 8d 35 29 5f f7 ff 	lea    -0x8a0d7(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9e8c9:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9e8ce:	b9 c9 02 00 00       	mov    $0x2c9,%ecx
   9e8d3:	41 b8 1d 00 00 00    	mov    $0x1d,%r8d
   9e8d9:	bf 01 00 00 00       	mov    $0x1,%edi
   9e8de:	e8 ed 80 08 00       	call   1269d0 <__wolf_rt_trap_at>
   9e8e3:	48 8d 35 08 5f f7 ff 	lea    -0x8a0f8(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9e8ea:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9e8ef:	b9 34 02 00 00       	mov    $0x234,%ecx
   9e8f4:	e9 c5 fc ff ff       	jmp    9e5be <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4fbe>
   9e8f9:	48 8d 35 f2 5e f7 ff 	lea    -0x8a10e(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9e900:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9e905:	b9 52 02 00 00       	mov    $0x252,%ecx
   9e90a:	41 b8 15 00 00 00    	mov    $0x15,%r8d
   9e910:	bf 04 00 00 00       	mov    $0x4,%edi
   9e915:	e8 b6 80 08 00       	call   1269d0 <__wolf_rt_trap_at>
   9e91a:	48 8d 35 d1 5e f7 ff 	lea    -0x8a12f(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9e921:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9e926:	b9 e0 02 00 00       	mov    $0x2e0,%ecx
   9e92b:	41 b8 19 00 00 00    	mov    $0x19,%r8d
   9e931:	bf 01 00 00 00       	mov    $0x1,%edi
   9e936:	e8 95 80 08 00       	call   1269d0 <__wolf_rt_trap_at>
   9e93b:	48 8d 35 b0 5e f7 ff 	lea    -0x8a150(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9e942:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9e947:	b9 a3 02 00 00       	mov    $0x2a3,%ecx
   9e94c:	41 b8 4a 00 00 00    	mov    $0x4a,%r8d
   9e952:	bf 01 00 00 00       	mov    $0x1,%edi
   9e957:	e8 74 80 08 00       	call   1269d0 <__wolf_rt_trap_at>
   9e95c:	48 8d 35 8f 5e f7 ff 	lea    -0x8a171(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9e963:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9e968:	b9 a2 02 00 00       	mov    $0x2a2,%ecx
   9e96d:	41 b8 2f 00 00 00    	mov    $0x2f,%r8d
   9e973:	bf 01 00 00 00       	mov    $0x1,%edi
   9e978:	e8 53 80 08 00       	call   1269d0 <__wolf_rt_trap_at>
   9e97d:	48 8d 35 6e 5e f7 ff 	lea    -0x8a192(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9e984:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9e989:	b9 51 02 00 00       	mov    $0x251,%ecx
   9e98e:	e9 2b fc ff ff       	jmp    9e5be <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4fbe>
   9e993:	48 8d 35 58 5e f7 ff 	lea    -0x8a1a8(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9e99a:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9e99f:	b9 47 02 00 00       	mov    $0x247,%ecx
   9e9a4:	e9 15 fc ff ff       	jmp    9e5be <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4fbe>
   9e9a9:	48 8d 35 42 5e f7 ff 	lea    -0x8a1be(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9e9b0:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9e9b5:	b9 44 02 00 00       	mov    $0x244,%ecx
   9e9ba:	41 b8 18 00 00 00    	mov    $0x18,%r8d
   9e9c0:	bf 03 00 00 00       	mov    $0x3,%edi
   9e9c5:	e8 06 80 08 00       	call   1269d0 <__wolf_rt_trap_at>
   9e9ca:	48 8d 35 21 5e f7 ff 	lea    -0x8a1df(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9e9d1:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9e9d6:	b9 4d 02 00 00       	mov    $0x24d,%ecx
   9e9db:	e9 de fb ff ff       	jmp    9e5be <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4fbe>
   9e9e0:	48 8d 35 0b 5e f7 ff 	lea    -0x8a1f5(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9e9e7:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9e9ec:	b9 a3 02 00 00       	mov    $0x2a3,%ecx
   9e9f1:	41 b8 42 00 00 00    	mov    $0x42,%r8d
   9e9f7:	bf 01 00 00 00       	mov    $0x1,%edi
   9e9fc:	e8 cf 7f 08 00       	call   1269d0 <__wolf_rt_trap_at>
   9ea01:	48 8d 35 ea 5d f7 ff 	lea    -0x8a216(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9ea08:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9ea0d:	b9 c4 02 00 00       	mov    $0x2c4,%ecx
   9ea12:	e9 ea fc ff ff       	jmp    9e701 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5101>
   9ea17:	48 8d 35 d4 5d f7 ff 	lea    -0x8a22c(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9ea1e:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9ea23:	b9 67 02 00 00       	mov    $0x267,%ecx
   9ea28:	e9 91 fb ff ff       	jmp    9e5be <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4fbe>
   9ea2d:	48 8d 35 be 5d f7 ff 	lea    -0x8a242(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9ea34:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9ea39:	b9 d8 02 00 00       	mov    $0x2d8,%ecx
   9ea3e:	e9 be fc ff ff       	jmp    9e701 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5101>
   9ea43:	48 8d 35 a8 5d f7 ff 	lea    -0x8a258(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9ea4a:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9ea4f:	b9 58 02 00 00       	mov    $0x258,%ecx
   9ea54:	41 b8 1d 00 00 00    	mov    $0x1d,%r8d
   9ea5a:	bf 03 00 00 00       	mov    $0x3,%edi
   9ea5f:	e8 6c 7f 08 00       	call   1269d0 <__wolf_rt_trap_at>
   9ea64:	48 8d 35 87 5d f7 ff 	lea    -0x8a279(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9ea6b:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9ea70:	b9 5d 02 00 00       	mov    $0x25d,%ecx
   9ea75:	41 b8 4b 00 00 00    	mov    $0x4b,%r8d
   9ea7b:	bf 03 00 00 00       	mov    $0x3,%edi
   9ea80:	e8 4b 7f 08 00       	call   1269d0 <__wolf_rt_trap_at>
   9ea85:	48 8d 35 66 5d f7 ff 	lea    -0x8a29a(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9ea8c:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9ea91:	b9 ea 02 00 00       	mov    $0x2ea,%ecx
   9ea96:	41 b8 25 00 00 00    	mov    $0x25,%r8d
   9ea9c:	bf 01 00 00 00       	mov    $0x1,%edi
   9eaa1:	e8 2a 7f 08 00       	call   1269d0 <__wolf_rt_trap_at>
   9eaa6:	48 8d 35 45 5d f7 ff 	lea    -0x8a2bb(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9eaad:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9eab2:	b9 5d 02 00 00       	mov    $0x25d,%ecx
   9eab7:	41 b8 27 00 00 00    	mov    $0x27,%r8d
   9eabd:	bf 03 00 00 00       	mov    $0x3,%edi
   9eac2:	e8 09 7f 08 00       	call   1269d0 <__wolf_rt_trap_at>
   9eac7:	48 8d 35 24 5d f7 ff 	lea    -0x8a2dc(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9eace:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9ead3:	b9 8b 02 00 00       	mov    $0x28b,%ecx
   9ead8:	e9 e1 fa ff ff       	jmp    9e5be <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4fbe>
   9eadd:	48 8d 35 0e 5d f7 ff 	lea    -0x8a2f2(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9eae4:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9eae9:	b9 94 02 00 00       	mov    $0x294,%ecx
   9eaee:	e9 68 03 00 00       	jmp    9ee5b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x585b>
   9eaf3:	48 8d 35 f8 5c f7 ff 	lea    -0x8a308(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9eafa:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9eaff:	b9 cd 02 00 00       	mov    $0x2cd,%ecx
   9eb04:	e9 52 03 00 00       	jmp    9ee5b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x585b>
   9eb09:	48 8d 35 e2 5c f7 ff 	lea    -0x8a31e(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9eb10:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9eb15:	b9 6a 02 00 00       	mov    $0x26a,%ecx
   9eb1a:	e9 0c fe ff ff       	jmp    9e92b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x532b>
   9eb1f:	48 8d 35 cc 5c f7 ff 	lea    -0x8a334(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9eb26:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9eb2b:	b9 5d 03 00 00       	mov    $0x35d,%ecx
   9eb30:	e9 cc fb ff ff       	jmp    9e701 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5101>
   9eb35:	48 8d 35 b6 5c f7 ff 	lea    -0x8a34a(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9eb3c:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9eb41:	b9 02 03 00 00       	mov    $0x302,%ecx
   9eb46:	e9 88 fd ff ff       	jmp    9e8d3 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x52d3>
   9eb4b:	48 8d 35 a0 5c f7 ff 	lea    -0x8a360(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9eb52:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9eb57:	b9 d5 02 00 00       	mov    $0x2d5,%ecx
   9eb5c:	e9 fa 02 00 00       	jmp    9ee5b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x585b>
   9eb61:	48 8d 35 8a 5c f7 ff 	lea    -0x8a376(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9eb68:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9eb6d:	b9 77 02 00 00       	mov    $0x277,%ecx
   9eb72:	e9 5c fd ff ff       	jmp    9e8d3 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x52d3>
   9eb77:	48 8d 35 74 5c f7 ff 	lea    -0x8a38c(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9eb7e:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9eb83:	b9 cb 02 00 00       	mov    $0x2cb,%ecx
   9eb88:	e9 c7 fe ff ff       	jmp    9ea54 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5454>
   9eb8d:	48 8d 35 5e 5c f7 ff 	lea    -0x8a3a2(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9eb94:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9eb99:	b9 8e 03 00 00       	mov    $0x38e,%ecx
   9eb9e:	e9 5e fb ff ff       	jmp    9e701 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5101>
   9eba3:	48 8d 35 48 5c f7 ff 	lea    -0x8a3b8(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9ebaa:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9ebaf:	b9 a3 02 00 00       	mov    $0x2a3,%ecx
   9ebb4:	e9 9b fe ff ff       	jmp    9ea54 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5454>
   9ebb9:	48 8d 35 32 5c f7 ff 	lea    -0x8a3ce(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9ebc0:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9ebc5:	b9 df 02 00 00       	mov    $0x2df,%ecx
   9ebca:	e9 8c 02 00 00       	jmp    9ee5b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x585b>
   9ebcf:	48 8d 35 1c 5c f7 ff 	lea    -0x8a3e4(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9ebd6:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9ebdb:	b9 6e 03 00 00       	mov    $0x36e,%ecx
   9ebe0:	e9 1c fb ff ff       	jmp    9e701 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5101>
   9ebe5:	48 8d 35 06 5c f7 ff 	lea    -0x8a3fa(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9ebec:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9ebf1:	b9 e0 02 00 00       	mov    $0x2e0,%ecx
   9ebf6:	e9 60 02 00 00       	jmp    9ee5b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x585b>
   9ebfb:	48 8d 35 f0 5b f7 ff 	lea    -0x8a410(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9ec02:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9ec07:	b9 91 03 00 00       	mov    $0x391,%ecx
   9ec0c:	e9 1a fd ff ff       	jmp    9e92b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x532b>
   9ec11:	48 8d 35 da 5b f7 ff 	lea    -0x8a426(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9ec18:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9ec1d:	b9 51 03 00 00       	mov    $0x351,%ecx
   9ec22:	e9 da fa ff ff       	jmp    9e701 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5101>
   9ec27:	48 8d 35 c4 5b f7 ff 	lea    -0x8a43c(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9ec2e:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9ec33:	b9 74 03 00 00       	mov    $0x374,%ecx
   9ec38:	e9 c4 fa ff ff       	jmp    9e701 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5101>
   9ec3d:	48 8d 35 ae 5b f7 ff 	lea    -0x8a452(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9ec44:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9ec49:	b9 1f 03 00 00       	mov    $0x31f,%ecx
   9ec4e:	e9 6b f9 ff ff       	jmp    9e5be <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4fbe>
   9ec53:	48 8d 35 98 5b f7 ff 	lea    -0x8a468(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9ec5a:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9ec5f:	b9 66 03 00 00       	mov    $0x366,%ecx
   9ec64:	e9 55 f9 ff ff       	jmp    9e5be <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4fbe>
   9ec69:	48 8d 35 82 5b f7 ff 	lea    -0x8a47e(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9ec70:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9ec75:	b9 42 03 00 00       	mov    $0x342,%ecx
   9ec7a:	41 b8 21 00 00 00    	mov    $0x21,%r8d
   9ec80:	bf 01 00 00 00       	mov    $0x1,%edi
   9ec85:	e8 46 7d 08 00       	call   1269d0 <__wolf_rt_trap_at>
   9ec8a:	48 8d 35 61 5b f7 ff 	lea    -0x8a49f(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9ec91:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9ec96:	b9 a7 02 00 00       	mov    $0x2a7,%ecx
   9ec9b:	e9 1e f9 ff ff       	jmp    9e5be <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4fbe>
   9eca0:	48 8d 35 4b 5b f7 ff 	lea    -0x8a4b5(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9eca7:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9ecac:	b9 2b 03 00 00       	mov    $0x32b,%ecx
   9ecb1:	e9 f2 01 00 00       	jmp    9eea8 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x58a8>
   9ecb6:	48 8d 35 35 5b f7 ff 	lea    -0x8a4cb(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9ecbd:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9ecc2:	b9 42 03 00 00       	mov    $0x342,%ecx
   9ecc7:	41 b8 21 00 00 00    	mov    $0x21,%r8d
   9eccd:	bf 04 00 00 00       	mov    $0x4,%edi
   9ecd2:	e8 f9 7c 08 00       	call   1269d0 <__wolf_rt_trap_at>
   9ecd7:	48 8d 35 14 5b f7 ff 	lea    -0x8a4ec(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9ecde:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9ece3:	b9 77 02 00 00       	mov    $0x277,%ecx
   9ece8:	e9 67 fd ff ff       	jmp    9ea54 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5454>
   9eced:	48 8d 35 fe 5a f7 ff 	lea    -0x8a502(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9ecf4:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9ecf9:	b9 6a 02 00 00       	mov    $0x26a,%ecx
   9ecfe:	e9 58 01 00 00       	jmp    9ee5b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x585b>
   9ed03:	48 8d 35 e8 5a f7 ff 	lea    -0x8a518(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9ed0a:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9ed0f:	b9 be 02 00 00       	mov    $0x2be,%ecx
   9ed14:	e9 a5 f8 ff ff       	jmp    9e5be <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4fbe>
   9ed19:	48 8d 35 d2 5a f7 ff 	lea    -0x8a52e(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9ed20:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9ed25:	b9 1c 03 00 00       	mov    $0x31c,%ecx
   9ed2a:	e9 8f f8 ff ff       	jmp    9e5be <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4fbe>
   9ed2f:	48 8d 35 bc 5a f7 ff 	lea    -0x8a544(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9ed36:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9ed3b:	b9 91 03 00 00       	mov    $0x391,%ecx
   9ed40:	e9 16 01 00 00       	jmp    9ee5b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x585b>
   9ed45:	48 8d 35 a6 5a f7 ff 	lea    -0x8a55a(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9ed4c:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9ed51:	b9 53 03 00 00       	mov    $0x353,%ecx
   9ed56:	e9 4d 01 00 00       	jmp    9eea8 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x58a8>
   9ed5b:	48 8d 35 90 5a f7 ff 	lea    -0x8a570(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9ed62:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9ed67:	b9 8e 03 00 00       	mov    $0x38e,%ecx
   9ed6c:	e9 4d f8 ff ff       	jmp    9e5be <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4fbe>
   9ed71:	48 8d 35 7a 5a f7 ff 	lea    -0x8a586(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9ed78:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9ed7d:	b9 a2 02 00 00       	mov    $0x2a2,%ecx
   9ed82:	41 b8 2f 00 00 00    	mov    $0x2f,%r8d
   9ed88:	bf 03 00 00 00       	mov    $0x3,%edi
   9ed8d:	e8 3e 7c 08 00       	call   1269d0 <__wolf_rt_trap_at>
   9ed92:	48 8d 35 59 5a f7 ff 	lea    -0x8a5a7(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9ed99:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9ed9e:	b9 bf 02 00 00       	mov    $0x2bf,%ecx
   9eda3:	e9 62 fb ff ff       	jmp    9e90a <_Wfl.fl_vm_exec$a5081fe92876eb37+0x530a>
   9eda8:	48 8d 35 43 5a f7 ff 	lea    -0x8a5bd(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9edaf:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9edb4:	b9 bf 02 00 00       	mov    $0x2bf,%ecx
   9edb9:	e9 00 f8 ff ff       	jmp    9e5be <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4fbe>
   9edbe:	48 8d 35 2d 5a f7 ff 	lea    -0x8a5d3(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9edc5:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9edca:	b9 c1 02 00 00       	mov    $0x2c1,%ecx
   9edcf:	e9 36 fb ff ff       	jmp    9e90a <_Wfl.fl_vm_exec$a5081fe92876eb37+0x530a>
   9edd4:	48 8d 35 17 5a f7 ff 	lea    -0x8a5e9(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9eddb:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9ede0:	b9 6a 02 00 00       	mov    $0x26a,%ecx
   9ede5:	e9 be 00 00 00       	jmp    9eea8 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x58a8>
   9edea:	48 8d 35 01 5a f7 ff 	lea    -0x8a5ff(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9edf1:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9edf6:	b9 77 02 00 00       	mov    $0x277,%ecx
   9edfb:	eb 3d                	jmp    9ee3a <_Wfl.fl_vm_exec$a5081fe92876eb37+0x583a>
   9edfd:	48 8d 35 ee 59 f7 ff 	lea    -0x8a612(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9ee04:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9ee09:	b9 02 03 00 00       	mov    $0x302,%ecx
   9ee0e:	e9 41 fc ff ff       	jmp    9ea54 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5454>
   9ee13:	48 8d 35 d8 59 f7 ff 	lea    -0x8a628(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9ee1a:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9ee1f:	b9 c2 02 00 00       	mov    $0x2c2,%ecx
   9ee24:	e9 95 f7 ff ff       	jmp    9e5be <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4fbe>
   9ee29:	48 8d 35 c2 59 f7 ff 	lea    -0x8a63e(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9ee30:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9ee35:	b9 02 03 00 00       	mov    $0x302,%ecx
   9ee3a:	41 b8 1d 00 00 00    	mov    $0x1d,%r8d
   9ee40:	bf 04 00 00 00       	mov    $0x4,%edi
   9ee45:	e8 86 7b 08 00       	call   1269d0 <__wolf_rt_trap_at>
   9ee4a:	48 8d 35 a1 59 f7 ff 	lea    -0x8a65f(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9ee51:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9ee56:	b9 7c 03 00 00       	mov    $0x37c,%ecx
   9ee5b:	41 b8 19 00 00 00    	mov    $0x19,%r8d
   9ee61:	bf 03 00 00 00       	mov    $0x3,%edi
   9ee66:	e8 65 7b 08 00       	call   1269d0 <__wolf_rt_trap_at>
   9ee6b:	48 8d 35 80 59 f7 ff 	lea    -0x8a680(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9ee72:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9ee77:	b9 b5 02 00 00       	mov    $0x2b5,%ecx
   9ee7c:	e9 3d f7 ff ff       	jmp    9e5be <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4fbe>
   9ee81:	48 8d 35 6a 59 f7 ff 	lea    -0x8a696(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9ee88:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9ee8d:	b9 51 03 00 00       	mov    $0x351,%ecx
   9ee92:	e9 27 f7 ff ff       	jmp    9e5be <_Wfl.fl_vm_exec$a5081fe92876eb37+0x4fbe>
   9ee97:	48 8d 35 54 59 f7 ff 	lea    -0x8a6ac(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9ee9e:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9eea3:	b9 f1 02 00 00       	mov    $0x2f1,%ecx
   9eea8:	41 b8 19 00 00 00    	mov    $0x19,%r8d
   9eeae:	bf 04 00 00 00       	mov    $0x4,%edi
   9eeb3:	e8 18 7b 08 00       	call   1269d0 <__wolf_rt_trap_at>
   9eeb8:	48 8d 35 33 59 f7 ff 	lea    -0x8a6cd(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9eebf:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9eec4:	b9 7c 03 00 00       	mov    $0x37c,%ecx
   9eec9:	e9 5d fa ff ff       	jmp    9e92b <_Wfl.fl_vm_exec$a5081fe92876eb37+0x532b>
   9eece:	48 8d 35 1d 59 f7 ff 	lea    -0x8a6e3(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9eed5:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9eeda:	b9 42 03 00 00       	mov    $0x342,%ecx
   9eedf:	41 b8 21 00 00 00    	mov    $0x21,%r8d
   9eee5:	bf 03 00 00 00       	mov    $0x3,%edi
   9eeea:	e8 e1 7a 08 00       	call   1269d0 <__wolf_rt_trap_at>
   9eeef:	48 8d 35 fc 58 f7 ff 	lea    -0x8a704(%rip),%rsi        # 147f2 <__abi_tag+0x144f6>
   9eef6:	ba 2e 00 00 00       	mov    $0x2e,%edx
   9eefb:	b9 7a 02 00 00       	mov    $0x27a,%ecx
   9ef00:	e9 4f fb ff ff       	jmp    9ea54 <_Wfl.fl_vm_exec$a5081fe92876eb37+0x5454>
   9ef05:	66 66 2e 0f 1f 84 00 	data16 cs nopw 0x0(%rax,%rax,1)
   9ef0c:	00 00 00 00 
