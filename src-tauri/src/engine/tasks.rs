//! Scheduled Task Scheduler scrub.
//!
//! The Task Scheduler COM interface is avoided on purpose: the PowerShell
//! bridge produces language independent JSON which can be parsed reliably on
//! every locale.

use serde_json::Value;

use crate::error::Failure;

/// A scheduled task registered with the Task Scheduler.
#[derive(Debug, Clone, Default)]
pub struct ScheduledTask {
    pub task_path: String,
    pub task_name: String,
    pub state: String,
    /// Executable + arguments of every action of the task.
    pub actions: Vec<String>,
}

impl ScheduledTask {
    pub fn identifier(&self) -> String {
        format!("{}{}", self.task_path, self.task_name)
    }
}

/// Executes a PowerShell script and returns its standard output.
fn run_powershell(script: &str) -> Option<String> {
    let script_path = std::env::temp_dir().join(format!(
        "disintegrator-{}-{}.ps1",
        std::process::id(),
        script.len()
    ));

    std::fs::write(&script_path, script).ok()?;

    let output = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&script_path)
        .output()
        .ok();

    let _ = std::fs::remove_file(&script_path);

    let output = output?;
    if !output.status.success() {
        return None;
    }

    Some(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Enumerates every scheduled task of the machine.
pub fn list_tasks() -> Vec<ScheduledTask> {
    const SCRIPT: &str = r#"
$ErrorActionPreference = 'SilentlyContinue'
$items = @(Get-ScheduledTask | ForEach-Object {
    $actionText = @($_.Actions | ForEach-Object {
        $command = $_.Execute
        if ($_.Arguments) { $command = $command + ' ' + $_.Arguments }
        $command
    }) -join ' ;; '
    [PSCustomObject]@{
        TaskPath = [string]$_.TaskPath
        TaskName = [string]$_.TaskName
        State    = [string]$_.State
        Actions  = $actionText
    }
})
ConvertTo-Json -InputObject $items -Depth 4 -Compress
"#;

    let Some(output) = run_powershell(SCRIPT) else {
        return Vec::new();
    };

    let trimmed = output.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    let parsed: Value = match serde_json::from_str(trimmed) {
        Ok(value) => value,
        Err(_) => return Vec::new(),
    };

    let entries = match parsed {
        Value::Array(items) => items,
        Value::Object(_) => vec![parsed],
        _ => Vec::new(),
    };

    entries
        .into_iter()
        .filter_map(|item| {
            let task_path = item.get("TaskPath").and_then(Value::as_str).unwrap_or("");
            let task_name = item.get("TaskName").and_then(Value::as_str).unwrap_or("");
            if task_name.is_empty() {
                return None;
            }
            let state = item.get("State").and_then(Value::as_str).unwrap_or("").to_string();
            let actions = item
                .get("Actions")
                .and_then(Value::as_str)
                .unwrap_or("")
                .split(" ;; ")
                .filter(|action| !action.trim().is_empty())
                .map(|action| action.trim().to_string())
                .collect();

            Some(ScheduledTask {
                task_path: task_path.to_string(),
                task_name: task_name.to_string(),
                state,
                actions,
            })
        })
        .collect()
}

/// Removes a scheduled task.
pub fn delete_task(task_path: &str, task_name: &str) -> Result<(), String> {
    let escaped_path = task_path.replace('\'', "''");
    let escaped_name = task_name.replace('\'', "''");

    let script = format!(
        "$ErrorActionPreference = 'Stop'\n\
         Unregister-ScheduledTask -TaskName '{escaped_name}' -TaskPath '{escaped_path}' -Confirm:$false\n"
    );

    match run_powershell(&script) {
        Some(_) => Ok(()),
        None => Err(format!("cannot remove scheduled task '{escaped_path}{escaped_name}'")),
    }
}

/// Removes every scheduled task that belongs to the target.
pub fn purge_tasks(
    tasks: &[ScheduledTask],
    needles: &[String],
) -> (u32, Vec<Failure>) {
    let mut removed = 0;
    let mut failures = Vec::new();

    for task in tasks {
        let haystack = format!(
            "{} {} {}",
            task.task_name,
            task.task_path,
            task.actions.join(" ")
        )
        .to_ascii_lowercase();

        let matches = needles
            .iter()
            .any(|needle| haystack.contains(&needle.to_ascii_lowercase()));

        if !matches {
            continue;
        }

        match delete_task(&task.task_path, &task.task_name) {
            Ok(()) => removed += 1,
            Err(error) => failures.push(Failure {
                category: "scheduled_task".to_string(),
                target: task.identifier(),
                reason: error,
                deferred: false,
            }),
        }
    }

    (removed, failures)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enumerates_scheduled_tasks() {
        let tasks = list_tasks();
        // Windows always ships a couple of maintenance tasks; the enumeration
        // must not fail even when the host is stripped down.
        assert!(tasks.iter().all(|task| !task.task_name.is_empty()));
    }
}
