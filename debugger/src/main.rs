use anyhow::Context;
use std::io::Read;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use windows::core::PCSTR;
use windows::Win32::Foundation::HINSTANCE;
use windows::Win32::System::LibraryLoader::FreeLibrary;
use windows::Win32::System::LibraryLoader::GetProcAddress;
use windows::Win32::System::LibraryLoader::LoadLibraryA;
use windows::Win32::UI::WindowsAndMessaging::GetMessageA;
use windows::Win32::UI::WindowsAndMessaging::SetTimer;
use windows::Win32::UI::WindowsAndMessaging::SetWindowsHookExA;
use windows::Win32::UI::WindowsAndMessaging::UnhookWindowsHookEx;
use windows::Win32::UI::WindowsAndMessaging::HHOOK;
use windows::Win32::UI::WindowsAndMessaging::HOOKPROC;
use windows::Win32::UI::WindowsAndMessaging::MSG;
use windows::Win32::UI::WindowsAndMessaging::WH_DEBUG;

#[derive(Debug)]
struct HookLibrary {
    handle: HINSTANCE,
}

struct HookFunction<'a> {
    _library: &'a HookLibrary,
    hook_proc: HOOKPROC,
}

impl<'a> HookFunction<'a> {
    fn get(&self) -> HOOKPROC {
        self.hook_proc
    }
}

impl HookLibrary {
    fn load() -> anyhow::Result<HookLibrary> {
        let handle = unsafe {
            LoadLibraryA(PCSTR("hook_debug.dll\0".as_ptr())).context("failed to load hook DLL")?
        };
        tracing::debug!("loaded hook dll");
        Ok(HookLibrary { handle })
    }

    fn get_module_handle(&self) -> HINSTANCE {
        self.handle
    }

    fn get_hook_address(&self) -> anyhow::Result<HookFunction<'_>> {
        let proc_address = unsafe {
            GetProcAddress(self.handle, PCSTR("debug_hook\0".as_ptr()))
                .context("failed to load debug hook function")?
        };
        tracing::debug!("got debug_hook address");
        Ok(HookFunction {
            _library: self,
            hook_proc: Some(unsafe { std::mem::transmute(proc_address) }),
        })
    }
}

impl Drop for HookLibrary {
    fn drop(&mut self) {
        assert!(!self.handle.is_invalid());
        unsafe {
            FreeLibrary(self.handle)
                .ok()
                .unwrap_or_else(|e| tracing::warn!("failed to free hook library: {}", e))
        }
        tracing::debug!("dropped hook library");
    }
}

#[derive(Debug)]
struct DebugHook {
    _library: HookLibrary,
    handle: HHOOK,
}

impl DebugHook {
    fn install() -> anyhow::Result<DebugHook> {
        let library = HookLibrary::load().context("failed to load hook library")?;
        let hook_fn = library
            .get_hook_address()
            .context("failed to find hook function")?;

        let handle = unsafe {
            SetWindowsHookExA(WH_DEBUG, hook_fn.get(), library.get_module_handle(), 0)
                .context("failed to install debug hook")?
        };

        tracing::info!("installed debug hook");

        Ok(DebugHook {
            _library: library,
            handle,
        })
    }
}

impl Drop for DebugHook {
    fn drop(&mut self) {
        assert!(!self.handle.is_invalid());
        unsafe {
            UnhookWindowsHookEx(self.handle)
                .ok()
                .unwrap_or_else(|e| tracing::warn!("failed to uninstall debug hook: {}", e))
        }

        tracing::info!("uninstalled debug hook");
    }
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let quit = Arc::new(AtomicBool::new(false));

    std::thread::scope(|s| {
        let _quit_thread = s.spawn(|| {
            println!("Press enter to quit");
            std::io::stdin().read_exact(&mut [0]).unwrap();
            tracing::info!("user requested quit");
            quit.store(true, Ordering::Release);
        });

        let _hook = DebugHook::install()?;

        // Needed so the message loop will periodically check to see if `quit` has been set
        unsafe {
            SetTimer(None, 0, 1000, None);
        }

        let mut msg = MSG::default();
        loop {
            if quit.load(Ordering::Acquire) {
                break;
            }

            let msg_result = unsafe { GetMessageA(&mut msg, None, 0, 0) };
            if msg_result.0 < 0 {
                anyhow::bail!("an error occurred in GetMessage");
            }

            tracing::trace!("received timer message");
        }

        Ok(())
    })
}
