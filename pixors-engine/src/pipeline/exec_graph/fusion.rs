use crate::gpu::kernel::{GpuKernel, KernelClass, KernelSig};

/// Fuses two adjacents kernels A and B into a chained kernel C.
/// Returns `None` if fusion rules are not satisfied.
pub fn try_fuse(a: &dyn GpuKernel, b: &dyn GpuKernel) -> Option<Box<dyn GpuKernel>> {
    let sig_a = a.sig();
    let sig_b = b.sig();

    // Only PerPixel + PerPixel for now
    if !matches!(sig_a.class, KernelClass::PerPixel)
        || !matches!(sig_b.class, KernelClass::PerPixel)
    {
        return None;
    }

    let fa = a.fusable_body()?;
    let fb = b.fusable_body()?;

    let fused_body = format!(
        "        // fused kernel: {} → {}\n        let pix0 = unpack(in0_src[idx]);\n        {}\n        {}\n        out0_dst[idx] = pack(pix2);\n",
        sig_a.name,
        sig_b.name,
        fa.replace("return", "pix1 ="),
        fb.replace("return", "pix2 ="),
    );

    let fused_sig = KernelSig {
        name: "fused",
        inputs: sig_a.inputs,
        outputs: sig_b.outputs,
        params: &[],
        workgroup: (8, 8, 1),
        dispatch: crate::gpu::kernel::DispatchShape::PerPixel,
        class: KernelClass::PerPixel,
        body_wgsl: Box::leak(fused_body.into_boxed_str()),
    };

    Some(Box::new(FusedKernel {
        sig: fused_sig,
        _a: a.sig().name,
        _b: b.sig().name,
    }))
}

struct FusedKernel {
    sig: KernelSig,
    _a: &'static str,
    _b: &'static str,
}

impl GpuKernel for FusedKernel {
    fn sig(&self) -> &KernelSig {
        &self.sig
    }

    fn write_params(&self, _dst: &mut [u8]) {}

    fn fusable_body(&self) -> Option<&'static str> {
        None // already fused
    }
}
