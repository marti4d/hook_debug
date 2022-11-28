use anyhow::Context;
use std::fs::File;
use std::io::Write;
use std::sync::Mutex;
use windows::core::PCSTR;
use windows::Win32::Foundation::LPARAM;
use windows::Win32::Foundation::LRESULT;
use windows::Win32::Foundation::WPARAM;
use windows::Win32::System::Diagnostics::Debug::OutputDebugStringA;
use windows::Win32::System::ProcessStatus::K32GetProcessImageFileNameA;
use windows::Win32::System::Threading::GetProcessIdOfThread;
use windows::Win32::System::Threading::OpenProcess;
use windows::Win32::System::Threading::OpenThread;
use windows::Win32::System::Threading::PROCESS_QUERY_LIMITED_INFORMATION;
use windows::Win32::System::Threading::THREAD_QUERY_LIMITED_INFORMATION;
use windows::Win32::UI::WindowsAndMessaging::CallNextHookEx;
use windows::Win32::UI::WindowsAndMessaging::DEBUGHOOKINFO;
use windows::Win32::UI::WindowsAndMessaging::WH_KEYBOARD;
use windows::Win32::UI::WindowsAndMessaging::WH_MOUSE;

static OUTPUT_FILE: Mutex<Option<File>> = Mutex::new(None);

#[derive(Debug)]
enum HookType {
    Mouse,
    Keyboard,
    Other,
}

impl HookType {
    fn from_wparam(wparam: WPARAM) -> HookType {
        if wparam.0 == WH_MOUSE.0 as usize {
            HookType::Mouse
        } else if wparam.0 == WH_KEYBOARD.0 as usize {
            HookType::Keyboard
        } else {
            HookType::Other
        }
    }
    fn should_log(&self) -> bool {
        match self {
            HookType::Mouse => true,
            HookType::Keyboard => true,
            HookType::Other => false,
        }
    }
}

/// # Safety
///
/// This function is safe as long as lparam is a valid pointer to a DEBUGHOOKINFO, which is
/// basically guaranteed by Windows
#[no_mangle]
pub unsafe extern "system" fn debug_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let hook_type = HookType::from_wparam(wparam);

    if hook_type.should_log() {
        let result = (|| -> anyhow::Result<()> {
            let hook_info = {
                let ptr = lparam.0 as *const DEBUGHOOKINFO;
                assert!(!ptr.is_null());
                &*ptr
            };

            let thread_id = hook_info.idThread;

            let mut locked_file = match OUTPUT_FILE.lock() {
                Ok(locked) => locked,
                Err(_) => anyhow::bail!("failed to lock mutex"),
            };

            if locked_file.is_none() {
                let file_path = std::env::temp_dir().join("input_events.txt");

                *locked_file = Some(
                    File::options()
                        .append(true)
                        .create(true)
                        .open(&file_path)
                        .context("failed to open file for writing")?,
                );
            }

            let thread_handle = OpenThread(THREAD_QUERY_LIMITED_INFORMATION, false, thread_id)
                .context("failed to open thread")?;

            let process_id = GetProcessIdOfThread(thread_handle);

            let process_handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id)
                .context("failed to open process")?;

            let mut process_name_buffer = vec![0u8; 400];
            let name_len =
                K32GetProcessImageFileNameA(process_handle, process_name_buffer.as_mut());
            if name_len == 0 {
                anyhow::bail!("failed to get process name");
            }

            process_name_buffer.truncate(name_len as usize);

            let process_name = String::from_utf8_lossy(&process_name_buffer);

            writeln!(
                locked_file.as_mut().unwrap(),
                "hook type: {:?}, process id: {}, process name: {}, thread id: {}",
                hook_type,
                process_id,
                process_name,
                thread_id,
            )
            .context("failed to write to file")?;

            Ok(())
        })();

        if let Err(e) = result {
            let msg = format!("an error occurred: {}\0", e);
            OutputDebugStringA(PCSTR(msg.as_ptr()));
        }
    }

    CallNextHookEx(None, code, wparam, lparam)
}
