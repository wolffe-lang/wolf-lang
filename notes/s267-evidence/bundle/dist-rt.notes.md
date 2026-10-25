<!-- wolf-rt-musl -->
### The musl runtime (x86_64-unknown-linux-musl)

`wolf-rt-0.2.35-x86_64-unknown-linux-musl.tar.gz` carries `libwolf_rt.a` for `x86_64-unknown-linux-musl`, the runtime a static link against musl needs, with `LICENSE` and the runtime's `LICENSE-EXCEPTION` (wolf-lang#935). It is built as PAX's `tools/mkwolf` builds its dev image's: rust 1.97.1, `--release --locked`, `+crt-static`, `relocation-model=static`, symbols stripped, the source, target and registry paths remapped. `libwolf_rt_none.a`, the freestanding runtime, is the one every host archive carries.

- `wolf-rt-0.2.35-x86_64-unknown-linux-musl.tar.gz` sha256 `a6d0bfb0a2770543fc8ed09dcb125fd8b84606e4adaa44bdd7df2c12c0e253d5`
- `libwolf_rt.a` sha256 `3d6f821ae70ebb40d106f31c6a95ee46c0c3626d9bf230f43c1f0b8c7c26eb24` (46699408 bytes)
