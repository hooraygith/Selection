use accessibility_ng::{AXAttribute, AXUIElement};
use accessibility_sys_ng::{kAXFocusedUIElementAttribute, kAXSelectedTextAttribute};
use core_foundation::string::CFString;
use core_graphics::event::{CGEvent, CGEventFlags, CGKeyCode};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use log::{error, info};
use objc::runtime::Object;
use std::error::Error;
use std::thread;
use std::time::{Duration, Instant};

pub fn get_text() -> String {
    // 1. 获取当前前台活跃的 App 包名
    let active_app_bundle_id = get_active_app_bundle_id().unwrap_or_default();

    // 2. 定义黑名单 (已知 AX 查询极慢或经常卡死的应用)
    let blacklist = vec![
        "com.google.Chrome",
        "com.microsoft.VSCode",
        "md.obsidian",
        "com.microsoft.edgemac",
        "com.brave.Browser",
        "com.avast.browser",
        "org.bitbrowser.BitBrowser",
        "org.chromium.Chromium",
        "ru.yandex.desktop.yandex-browser",
        "com.tencent.xinWeChat",
        "com.tencent.qq",
    ];

    info!("===========当前活跃 App: {} ===========", active_app_bundle_id);

    // 3. 命中黑名单，直接走最高效的原生剪贴板方案
    if blacklist.contains(&active_app_bundle_id.as_str()) {
        info!("命中黑名单，跳过 AX API，直接执行原生 Cmd+C");
        return get_text_by_native_cmd_c();
    }

    // 4. 白名单应用：优先尝试优雅的 Accessibility API
    info!("尝试使用 Accessibility API 获取选中文本");
    match get_selected_text_by_ax() {
        Ok(text) => {
            if !text.is_empty() {
                return text;
            } else {
                info!("AX API 返回空字符串");
            }
        }
        Err(err) => {
            error!("AX API 失败: {}", err);
        }
    }

    // 5. 终极兜底：原生 Cmd+C
    info!("AX 失败，Fallback 到原生 Cmd+C");
    get_text_by_native_cmd_c()
}

// 原有的 Accessibility API 获取文本实现
fn get_selected_text_by_ax() -> Result<String, Box<dyn Error>> {
    let system_element = AXUIElement::system_wide();
    let Some(selected_element) = system_element
        .attribute(&AXAttribute::new(&CFString::from_static_string(
            kAXFocusedUIElementAttribute,
        )))
        .map(|element| element.downcast_into::<AXUIElement>())
        .ok()
        .flatten()
    else {
        return Err(Box::new(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "No selected element",
        )));
    };
    let Some(selected_text) = selected_element
        .attribute(&AXAttribute::new(&CFString::from_static_string(
            kAXSelectedTextAttribute,
        )))
        .map(|text| text.downcast_into::<CFString>())
        .ok()
        .flatten()
    else {
        return Err(Box::new(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "No selected text",
        )));
    };
    Ok(selected_text.to_string())
}

// 获取当前活动窗口的 Bundle ID
fn get_active_app_bundle_id() -> Option<String> {
    unsafe {
        let workspace: *mut Object = msg_send![class!(NSWorkspace), sharedWorkspace];
        let front_app: *mut Object = msg_send![workspace, frontmostApplication];
        if front_app.is_null() {
            return None;
        }

        let bundle_id_nsstring: *mut Object = msg_send![front_app, bundleIdentifier];
        if bundle_id_nsstring.is_null() {
            return None;
        }

        let utf8_chars: *const i8 = msg_send![bundle_id_nsstring, UTF8String];
        let c_str = std::ffi::CStr::from_ptr(utf8_chars);
        Some(c_str.to_string_lossy().into_owned())
    }
}

// 使用原生 API 触发 Cmd+C 并轮询剪贴板
fn get_text_by_native_cmd_c() -> String {
    unsafe {
        let pasteboard: *mut Object = msg_send![class!(NSPasteboard), generalPasteboard];
        let initial_change_count: isize = msg_send![pasteboard, changeCount];

        simulate_cmd_c();

        let timeout = Duration::from_millis(150);
        let start_time = Instant::now();

        while start_time.elapsed() < timeout {
            let current_change_count: isize = msg_send![pasteboard, changeCount];

            if current_change_count > initial_change_count {
                let ns_string_class = class!(NSString);
                let string_type: *mut Object = msg_send![ns_string_class, stringWithUTF8String: b"public.utf8-plain-text\0".as_ptr()];

                let content: *mut Object = msg_send![pasteboard, stringForType: string_type];

                if !content.is_null() {
                    let utf8_chars: *const i8 = msg_send![content, UTF8String];
                    let c_str = std::ffi::CStr::from_ptr(utf8_chars);
                    return c_str.to_string_lossy().into_owned();
                }
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }

        info!("原生 Cmd+C 获取剪贴板超时或未选中内容");
        String::new()
    }
}

// 模拟发送 Cmd+C 按键事件
fn simulate_cmd_c() {
    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState).unwrap();
    let key_c: CGKeyCode = 8;

    let key_down = CGEvent::new_keyboard_event(source.clone(), key_c, true).unwrap();
    key_down.set_flags(CGEventFlags::CGEventFlagCommand);

    let key_up = CGEvent::new_keyboard_event(source, key_c, false).unwrap();
    key_up.set_flags(CGEventFlags::CGEventFlagCommand);

    key_down.post(core_graphics::event::CGEventTapLocation::HID);
    key_up.post(core_graphics::event::CGEventTapLocation::HID);
}