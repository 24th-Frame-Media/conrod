//! How much video memory the machine has, asked of DXGI so it works for any GPU
//! vendor before the (optional, multi-GB) model is downloaded.

#[derive(Debug, Clone, PartialEq)]
pub struct GpuMemory {
    pub name: String,
    /// Dedicated video memory, bytes.
    pub total: u64,
    /// What the OS budget lets this process still allocate, bytes (total minus
    /// what every process on the adapter already uses).
    pub free: u64,
}

/// The discrete (most dedicated memory) hardware adapter, or `None` without one.
#[cfg(windows)]
pub fn query() -> Option<GpuMemory> {
    use windows::core::Interface;
    use windows::Win32::Graphics::Dxgi::{
        CreateDXGIFactory1, IDXGIAdapter3, IDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE,
        DXGI_MEMORY_SEGMENT_GROUP_LOCAL, DXGI_QUERY_VIDEO_MEMORY_INFO,
    };
    unsafe {
        let factory: IDXGIFactory1 = CreateDXGIFactory1().ok()?;
        let mut best: Option<GpuMemory> = None;
        let mut i = 0;
        while let Ok(adapter) = factory.EnumAdapters1(i) {
            i += 1;
            let Ok(desc) = adapter.GetDesc1() else {
                continue;
            };
            if desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 != 0 {
                continue;
            }
            let total = desc.DedicatedVideoMemory as u64;
            let mut info = DXGI_QUERY_VIDEO_MEMORY_INFO::default();
            let free = match adapter.cast::<IDXGIAdapter3>() {
                Ok(a3)
                    if a3
                        .QueryVideoMemoryInfo(0, DXGI_MEMORY_SEGMENT_GROUP_LOCAL, &mut info)
                        .is_ok() =>
                {
                    info.Budget.saturating_sub(info.CurrentUsage).min(total)
                }
                _ => total,
            };
            let len = desc.Description.iter().position(|c| *c == 0).unwrap_or(128);
            let name = String::from_utf16_lossy(&desc.Description[..len]);
            if best.as_ref().is_none_or(|b| total > b.total) {
                best = Some(GpuMemory { name, total, free });
            }
        }
        best.filter(|b| b.total > 0)
    }
}

#[cfg(not(windows))]
pub fn query() -> Option<GpuMemory> {
    None
}
