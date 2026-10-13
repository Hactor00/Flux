param([Parameter(Mandatory=$true)][uint32]$BrowserProcessId)
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = New-Object System.Text.UTF8Encoding $false
Add-Type @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
public static class BrowserWindowInspector {
    public delegate bool Enumerate(IntPtr window, IntPtr parameter);
    [DllImport("user32.dll")] static extern bool EnumWindows(Enumerate callback, IntPtr parameter);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr window, out uint process);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetClassName(IntPtr window, StringBuilder text, int length);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetWindowText(IntPtr window, StringBuilder text, int length);
    [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr window);
    [DllImport("user32.dll")] static extern IntPtr GetForegroundWindow();
    public static string[] Dialogs(uint process) {
        var titles = new List<string>();
        EnumWindows((window, parameter) => {
            uint owner;
            GetWindowThreadProcessId(window, out owner);
            var type = new StringBuilder(256);
            GetClassName(window, type, type.Capacity);
            if (owner == process && type.ToString() == "#32770" && IsWindowVisible(window)) {
                var title = new StringBuilder(512);
                GetWindowText(window, title, title.Capacity);
                titles.Add(title.ToString());
            }
            return true;
        }, IntPtr.Zero);
        return titles.ToArray();
    }
    public static string ForegroundTitle() {
        var title = new StringBuilder(512);
        GetWindowText(GetForegroundWindow(), title, title.Capacity);
        return title.ToString();
    }
}
'@
@{ dialogs = @([BrowserWindowInspector]::Dialogs($BrowserProcessId)); foreground = [BrowserWindowInspector]::ForegroundTitle() } | ConvertTo-Json -Compress
