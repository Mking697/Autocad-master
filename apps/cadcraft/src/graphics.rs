//! Windows graphics instance configuration, applied before wgpu can initialize any driver.

use eframe::{
    NativeOptions,
    egui_wgpu::WgpuSetup,
    wgpu::{self, Backends, Dx12Compiler},
};

/// Default to DirectX 12 with FXC before creating the wgpu instance. Explicit
/// `WGPU_BACKEND` and `WGPU_DX12_COMPILER` choices, read by the caller, take precedence.
pub fn configure(options: &mut NativeOptions, backend_override: Option<Backends>, compiler_override: Option<Dx12Compiler>) {
    configure_with_probe(options, backend_override, compiler_override, probe_dx12_device_type);
}

/// `configure`, with the DX12 hardware-vs-software probe injected so tests can control it
/// without touching real graphics hardware.
fn configure_with_probe(
    options: &mut NativeOptions,
    backend_override: Option<Backends>,
    compiler_override: Option<Dx12Compiler>,
    probe: impl FnOnce() -> Option<wgpu::DeviceType>,
) {
    if let WgpuSetup::CreateNew(create) = &mut options.wgpu_options.wgpu_setup {
        // eframe's default includes GL. Creating that backend can crash inside AMD's
        // atio6axx.dll before adapter selection or any Rust error handling runs, so the
        // window never appears. Choosing an adapter afterwards is too late: only leaving GL
        // out of the instance avoids it. This matches the verified WGPU_BACKEND=dx12
        // workaround without mutating process environment variables.
        let backends = backend_override.unwrap_or_else(|| {
            // No explicit WGPU_BACKEND: some older iGPUs (e.g. Intel HD 4000-series/Haswell)
            // report a DX12 feature level below what Windows will hand a hardware adapter for,
            // so DXGI silently substitutes "Microsoft Basic Render Driver" (WARP, a CPU
            // rasterizer): CADCraft still runs, but every frame is software-rendered and the
            // canvas crawls while other apps (AutoCAD's lighter 2D GDI path) feel fine on the
            // same machine. GL has no such feature-level cutoff and drives this hardware for
            // real, so fall back to it when the DX12 probe can only find a software adapter.
            if probe() == Some(wgpu::DeviceType::Cpu) { Backends::GL } else { Backends::DX12 }
        });
        create.instance_descriptor.backends = backends;
        // Auto can load another program's dxcompiler.dll without a matching dxil.dll.
        // That fails wgpu's indirect-validation pipeline and loses the device before
        // the first frame (issue #608). FXC uses the Windows-provided compiler runtime.
        create.instance_descriptor.backend_options.dx12.shader_compiler = compiler_override.unwrap_or(Dx12Compiler::Fxc);
        log::info!(
            "Windows graphics: backends={:?}, DX12 compiler={:?}",
            create.instance_descriptor.backends,
            create.instance_descriptor.backend_options.dx12.shader_compiler
        );
    }
}

/// Synchronously asks DXGI for the adapter a DX12-only instance would get, and returns its
/// device type (`Cpu` means WARP / "Microsoft Basic Render Driver" — no real GPU behind it).
/// `None` means no DX12 adapter exists at all; `configure` keeps the DX12 default in that case
/// too (unchanged from before this probe existed), since GL may not fare any better.
fn probe_dx12_device_type() -> Option<wgpu::DeviceType> {
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor { backends: Backends::DX12, ..wgpu::InstanceDescriptor::new_without_display_handle() });
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).ok()?;
    Some(adapter.get_info().device_type)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn configured_backends(backend_override: Option<Backends>) -> Backends {
        let mut options = NativeOptions::default();
        configure(&mut options, backend_override, None);
        let WgpuSetup::CreateNew(create) = options.wgpu_options.wgpu_setup else {
            panic!("default native options must create a wgpu instance");
        };
        create.instance_descriptor.backends
    }

    fn configured_backends_with_probe(probe: impl FnOnce() -> Option<wgpu::DeviceType>) -> Backends {
        let mut options = NativeOptions::default();
        configure_with_probe(&mut options, None, None, probe);
        let WgpuSetup::CreateNew(create) = options.wgpu_options.wgpu_setup else {
            panic!("default native options must create a wgpu instance");
        };
        create.instance_descriptor.backends
    }

    #[test]
    fn windows_default_initializes_only_dx12() {
        // eframe's own default (PRIMARY | GL) would initialize OpenGL as well.
        let WgpuSetup::CreateNew(eframe_default) = NativeOptions::default().wgpu_options.wgpu_setup else {
            panic!("default native options must create a wgpu instance");
        };
        assert!(eframe_default.instance_descriptor.backends.contains(Backends::GL));
        assert_eq!(configured_backends(None), Backends::DX12);
    }

    #[test]
    fn falls_back_to_gl_when_the_only_dx12_adapter_is_software() {
        // Matches a real log seen on an Intel HD 4600 (Haswell): DXGI handed back "Microsoft
        // Basic Render Driver" (device_type: Cpu) for a DX12-only instance, even though the
        // machine has a real (if old) GPU and that GPU runs other apps' DX11/OpenGL fine.
        assert_eq!(configured_backends_with_probe(|| Some(wgpu::DeviceType::Cpu)), Backends::GL);
    }

    #[test]
    fn keeps_dx12_when_the_adapter_is_real_hardware() {
        for kind in [wgpu::DeviceType::DiscreteGpu, wgpu::DeviceType::IntegratedGpu, wgpu::DeviceType::VirtualGpu, wgpu::DeviceType::Other] {
            assert_eq!(configured_backends_with_probe(move || Some(kind)), Backends::DX12);
        }
    }

    #[test]
    fn keeps_dx12_when_no_adapter_is_found_at_all() {
        assert_eq!(configured_backends_with_probe(|| None), Backends::DX12);
    }

    #[test]
    fn explicit_backend_override_skips_the_probe_entirely() {
        // An explicit WGPU_BACKEND must win outright: the probe closure is never called.
        let backends = configured_backends_with_probe_override(Backends::VULKAN, || panic!("probe must not run"));
        assert_eq!(backends, Backends::VULKAN);
    }

    fn configured_backends_with_probe_override(backend_override: Backends, probe: impl FnOnce() -> Option<wgpu::DeviceType>) -> Backends {
        let mut options = NativeOptions::default();
        configure_with_probe(&mut options, Some(backend_override), None, probe);
        let WgpuSetup::CreateNew(create) = options.wgpu_options.wgpu_setup else {
            panic!("default native options must create a wgpu instance");
        };
        create.instance_descriptor.backends
    }

    #[test]
    fn windows_default_uses_fxc_instead_of_searching_for_dxc() {
        let mut options = NativeOptions::default();
        if let WgpuSetup::CreateNew(create) = &mut options.wgpu_options.wgpu_setup {
            // Auto can find an unrelated DXC without its matching DXIL runtime (issue #608).
            create.instance_descriptor.backend_options.dx12.shader_compiler = Dx12Compiler::Auto;
        }
        configure(&mut options, None, None);
        let WgpuSetup::CreateNew(create) = options.wgpu_options.wgpu_setup else {
            panic!("default native options must create a wgpu instance");
        };
        assert!(matches!(create.instance_descriptor.backend_options.dx12.shader_compiler, Dx12Compiler::Fxc));
    }

    #[test]
    fn explicit_compiler_and_backend_overrides_are_preserved() {
        let backends = Backends::VULKAN | Backends::DX12;
        for compiler in
            [Dx12Compiler::Fxc, Dx12Compiler::Auto, Dx12Compiler::StaticDxc, Dx12Compiler::DynamicDxc { dxc_path: "custom/dxcompiler.dll".into() }]
        {
            let mut options = NativeOptions::default();
            configure(&mut options, Some(backends), Some(compiler.clone()));
            let WgpuSetup::CreateNew(create) = options.wgpu_options.wgpu_setup else {
                panic!("default native options must create a wgpu instance");
            };
            assert_eq!(create.instance_descriptor.backends, backends);
            match (compiler, create.instance_descriptor.backend_options.dx12.shader_compiler) {
                (Dx12Compiler::Fxc, Dx12Compiler::Fxc)
                | (Dx12Compiler::Auto, Dx12Compiler::Auto)
                | (Dx12Compiler::StaticDxc, Dx12Compiler::StaticDxc) => {}
                (Dx12Compiler::DynamicDxc { dxc_path: expected }, Dx12Compiler::DynamicDxc { dxc_path: actual }) => {
                    assert_eq!(actual, expected);
                }
                (expected, actual) => panic!("compiler override changed from {expected:?} to {actual:?}"),
            }
        }
    }

    #[test]
    fn explicit_backend_override_is_preserved() {
        for backends in [Backends::VULKAN, Backends::GL, Backends::DX12, Backends::VULKAN | Backends::DX12, Backends::empty()] {
            assert_eq!(configured_backends(Some(backends)), backends);
        }
    }
}
