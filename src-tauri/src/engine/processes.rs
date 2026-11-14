//! Process & Service Neutraliser.
//!
//! Provides a snapshot of the running process table, resolution of the
//! processes that belong to a target and forceful termination of complete
//! process trees (a parent and every descendant it spawned).

use std::collections::HashMap;

use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
    TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, TerminateProcess,
    PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE,
};

use crate::model::InstalledApp;

/// Snapshot entry of a running process.
#[derive(Debug, Clone, Default)]
pub struct ProcessInfo {
    pub pid: u32,
    pub parent_pid: u32,
    pub name: String,
    pub executable: Option<String>,
}

/// Converts a UTF-16 buffer into a Rust string.
fn wide_to_string(buffer: &[u16]) -> String {
    let end = buffer.iter().position(|byte| *byte == 0).unwrap_or(buffer.len());
    String::from_utf16_lossy(&buffer[..end])
}

/// Enumerates the whole process table.
pub fn enumerate_processes() -> Vec<ProcessInfo> {
    let mut processes = Vec::new();

    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE || snapshot.is_null() {
        return processes;
    }

    let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
    entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;

    let mut has_entry = unsafe { Process32FirstW(snapshot, &mut entry) } != 0;
    while has_entry {
        let name = wide_to_string(&entry.szExeFile);
        let pid = entry.th32ProcessID;
        let parent_pid = entry.th32ParentProcessID;
        let executable = query_executable_path(pid);

        processes.push(ProcessInfo {
            pid,
            parent_pid,
            name,
            executable,
        });

        has_entry = unsafe { Process32NextW(snapshot, &mut entry) } != 0;
    }

    unsafe {
        CloseHandle(snapshot);
    }

    processes
}

/// Resolves the full image path of a process.
pub fn query_executable_path(pid: u32) -> Option<String> {
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return None;
    }

    let mut buffer = vec![0u16; 32768];
    let mut size = buffer.len() as u32;

    let ok = unsafe { QueryFullProcessImageNameW(handle, 0, buffer.as_mut_ptr(), &mut size) };

    unsafe {
        CloseHandle(handle);
    }

    if ok == 0 {
        return None;
    }

    let path = wide_to_string(&buffer[..size as usize]);
    if path.is_empty() {
        None
    } else {
        Some(path)
    }
}

/// Builds the parent -> children adjacency map of the process table.
pub fn build_tree(processes: &[ProcessInfo]) -> HashMap<u32, Vec<u32>> {
    let mut tree: HashMap<u32, Vec<u32>> = HashMap::new();
    for process in processes {
        tree.entry(process.parent_pid).or_default().push(process.pid);
    }
    tree
}

/// Collects the descendants of a process (depth first, cycle safe).
pub fn descendants_of(pid: u32, tree: &HashMap<u32, Vec<u32>>) -> Vec<u32> {
    let mut collected = Vec::new();
    let mut stack = vec![pid];
    let mut visited = std::collections::HashSet::new();

    while let Some(current) = stack.pop() {
        if !visited.insert(current) {
            continue;
        }
        if let Some(children) = tree.get(&current) {
            for child in children {
                stack.push(*child);
                collected.push(*child);
            }
        }
    }

    collected
}

/// Terminates a single process.
pub fn terminate_process(pid: u32) -> bool {
    let handle = unsafe { OpenProcess(PROCESS_TERMINATE, 0, pid) };
    if handle.is_null() {
        return false;
    }
    let ok = unsafe { TerminateProcess(handle, 1) };
    unsafe {
        CloseHandle(handle);
    }
    ok != 0
}

/// Terminates a process and every descendant it owns.
///
/// Children are killed first so that a supervising parent cannot respawn
/// them while the tree is being dismantled.
pub fn terminate_process_tree(pid: u32, tree: &HashMap<u32, Vec<u32>>) -> u32 {
    let mut terminated = 0;

    let descendants = descendants_of(pid, tree);
    for descendant in descendants.iter().rev() {
        if terminate_process(*descendant) {
            terminated += 1;
        }
    }

    if terminate_process(pid) {
        terminated += 1;
    }

    terminated
}

/// Finds the running processes that belong to the target application.
pub fn find_target_processes(app: &InstalledApp, processes: &[ProcessInfo]) -> Vec<ProcessInfo> {
    let install_root = app
        .install_location
        .as_ref()
        .map(|location| location.trim_end_matches('\\').to_ascii_lowercase());

    let executable = app
        .executable
        .as_ref()
        .map(|path| path.to_ascii_lowercase());

    let name_variants: Vec<String> = crate::engine::filesystem::name_variants(app);

    processes
        .iter()
        .filter(|process| {
            if let Some(root) = &install_root {
                if let Some(path) = &process.executable {
                    if path.to_ascii_lowercase().starts_with(root.as_str()) {
                        return true;
                    }
                }
            }

            if let Some(target) = &executable {
                if let Some(path) = &process.executable {
                    if path.to_ascii_lowercase() == *target {
                        return true;
                    }
                }
            }

            // Fall back to the process image name (no path available).
            if process.executable.is_none() {
                let lowered = process.name.to_ascii_lowercase();
                let stem = lowered.trim_end_matches(".exe").to_string();
                if name_variants
                    .iter()
                    .any(|variant| stem == *variant || stem.starts_with(variant.as_str()))
                {
                    return true;
                }
            }

            false
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enumerates_the_process_table() {
        let processes = enumerate_processes();
        assert!(!processes.is_empty(), "process table must not be empty");
        assert!(processes.iter().any(|process| process.pid > 0));
    }

    #[test]
    fn builds_and_walks_the_process_tree() {
        let processes = enumerate_processes();
        let tree = build_tree(&processes);

        let root = processes
            .iter()
            .find(|process| process.parent_pid != process.pid)
            .map(|process| process.pid);
        if let Some(root) = root {
            let descendants = descendants_of(root, &tree);
            assert!(
                !descendants.contains(&root),
                "root must not be its own descendant"
            );
        }
    }

    #[test]
    fn identifies_processes_by_installation_root() {
        let processes = enumerate_processes();
        let app = InstalledApp {
            id: "test".into(),
            display_name: "Contoso Suite".into(),
            display_version: None,
            publisher: None,
            install_date: None,
            install_location: Some(r"C:\Program Files\Contoso Suite".into()),
            uninstall_string: None,
            quiet_uninstall_string: None,
            estimated_size_bytes: None,
            system_component: false,
            windows_installer: false,
            parent_key_name: None,
            source: crate::model::AppSource::Registry,
            registry_key: None,
            executable: None,
            architecture: "x64".into(),
            tokens: Vec::new(),
        };

        let matches = find_target_processes(&app, &processes);
        assert!(matches.is_empty(), "no Contoso process may exist");
    }
}

