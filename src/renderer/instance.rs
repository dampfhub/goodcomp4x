use std::ffi::{CStr, c_char, c_void};

use anyhow::{Context, Result};
use ash::vk;
use raw_window_handle::RawDisplayHandle;

const VALIDATION_LAYER: &CStr = c"VK_LAYER_KHRONOS_validation";

/// Validation only runs in debug builds; release builds skip the overhead.
pub fn validation_enabled() -> bool {
    cfg!(debug_assertions)
}

pub unsafe fn create_entry() -> Result<ash::Entry> {
    unsafe { ash::Entry::load() }
        .context("failed to load the Vulkan loader (is a driver installed?)")
}

/// Creates the instance with the surface extensions the window needs, plus
/// validation in debug builds.
pub unsafe fn create_instance(
    entry: &ash::Entry,
    display_handle: RawDisplayHandle,
) -> Result<ash::Instance> {
    let app_info = vk::ApplicationInfo::default()
        .application_name(c"Hex Combat Sandbox")
        .api_version(vk::API_VERSION_1_3);

    let mut extensions = ash_window::enumerate_required_extensions(display_handle)?.to_vec();
    let mut layers: Vec<*const c_char> = Vec::new();
    if validation_enabled() {
        extensions.push(ash::ext::debug_utils::NAME.as_ptr());
        if layer_available(entry, VALIDATION_LAYER)? {
            layers.push(VALIDATION_LAYER.as_ptr());
        } else {
            log::warn!("validation layer not installed; continuing without it");
        }
    }

    let mut create_info = vk::InstanceCreateInfo::default()
        .application_info(&app_info)
        .enabled_extension_names(&extensions)
        .enabled_layer_names(&layers);

    // Chaining a messenger here also covers messages from instance creation
    // and destruction, which the standalone messenger can't see.
    let mut debug_info = debug_messenger_create_info();
    if !layers.is_empty() {
        create_info = create_info.push_next(&mut debug_info);
    }

    Ok(unsafe { entry.create_instance(&create_info, None) }?)
}

pub unsafe fn create_debug_messenger(
    entry: &ash::Entry,
    instance: &ash::Instance,
) -> Result<(ash::ext::debug_utils::Instance, vk::DebugUtilsMessengerEXT)> {
    let loader = ash::ext::debug_utils::Instance::new(entry, instance);
    let messenger =
        unsafe { loader.create_debug_utils_messenger(&debug_messenger_create_info(), None) }?;
    Ok((loader, messenger))
}

fn layer_available(entry: &ash::Entry, name: &CStr) -> Result<bool> {
    let layers = unsafe { entry.enumerate_instance_layer_properties() }?;
    Ok(layers
        .iter()
        .any(|layer| layer.layer_name_as_c_str() == Ok(name)))
}

fn debug_messenger_create_info<'a>() -> vk::DebugUtilsMessengerCreateInfoEXT<'a> {
    type Severity = vk::DebugUtilsMessageSeverityFlagsEXT;
    type Kind = vk::DebugUtilsMessageTypeFlagsEXT;

    vk::DebugUtilsMessengerCreateInfoEXT::default()
        .message_severity(Severity::VERBOSE | Severity::INFO | Severity::WARNING | Severity::ERROR)
        .message_type(Kind::GENERAL | Kind::VALIDATION | Kind::PERFORMANCE)
        .pfn_user_callback(Some(debug_callback))
}

/// Forwards validation messages to the `log` crate at the matching level.
unsafe extern "system" fn debug_callback(
    severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    kind: vk::DebugUtilsMessageTypeFlagsEXT,
    callback_data: *const vk::DebugUtilsMessengerCallbackDataEXT,
    _user_data: *mut c_void,
) -> vk::Bool32 {
    let message = unsafe { CStr::from_ptr((*callback_data).p_message) }.to_string_lossy();
    let level = match severity {
        vk::DebugUtilsMessageSeverityFlagsEXT::ERROR => log::Level::Error,
        vk::DebugUtilsMessageSeverityFlagsEXT::WARNING => log::Level::Warn,
        vk::DebugUtilsMessageSeverityFlagsEXT::INFO => log::Level::Info,
        _ => log::Level::Debug,
    };
    log::log!(level, "[vulkan:{kind:?}] {message}");
    vk::FALSE
}
