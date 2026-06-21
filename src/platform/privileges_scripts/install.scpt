on run {daemon_file, agent_file, user}

  set service_plist to "/Library/LaunchDaemons/com.carriez.RustDesk_service.plist"
  set agent_plist to "/Library/LaunchAgents/com.carriez.RustDesk_server.plist"

  set sh1 to "echo " & quoted form of daemon_file & " > " & quoted form of service_plist & " && chown root:wheel " & quoted form of service_plist & ";"

  set sh2 to "echo " & quoted form of agent_file & " > " & quoted form of agent_plist & " && chown root:wheel " & quoted form of agent_plist & ";"

  set root_prefs to "/var/root/Library/Preferences/com.carriez.RustDesk"
  set sh3 to "mkdir -p " & quoted form of root_prefs & "; cp -f /Users/" & user & "/Library/Preferences/com.carriez.RustDesk/RustDesk.toml " & quoted form of root_prefs & "/ 2>/dev/null || true;"

  set sh4 to "mkdir -p " & quoted form of root_prefs & "; cp -f /Users/" & user & "/Library/Preferences/com.carriez.RustDesk/RustDesk2.toml " & quoted form of root_prefs & "/ 2>/dev/null || true;"

  set resolve_uid to "uid=$(id -u " & quoted form of user & " 2>/dev/null || true);"
  set unload_agent to "if [ -n \"$uid\" ]; then launchctl bootout gui/$uid " & quoted form of agent_plist & " 2>/dev/null || launchctl bootout user/$uid " & quoted form of agent_plist & " 2>/dev/null || launchctl unload -w " & quoted form of agent_plist & " || true; else launchctl unload -w " & quoted form of agent_plist & " || true; fi;"
  set load_service to "launchctl unload -w " & quoted form of service_plist & " 2>/dev/null || true; launchctl load -w " & quoted form of service_plist & ";"
  set agent_label_cmd to "agent_label=$(basename " & quoted form of agent_plist & " .plist);"
  set bootstrap_agent to "if [ -n \"$uid\" ]; then launchctl bootstrap gui/$uid " & quoted form of agent_plist & " 2>/dev/null || launchctl bootstrap user/$uid " & quoted form of agent_plist & " 2>/dev/null || launchctl load -w " & quoted form of agent_plist & " || true; else launchctl load -w " & quoted form of agent_plist & " || true; fi;"
  set kickstart_agent to "if [ -n \"$uid\" ]; then launchctl kickstart -k gui/$uid/$agent_label 2>/dev/null || launchctl kickstart -k user/$uid/$agent_label 2>/dev/null || true; fi;"
  set load_agent to agent_label_cmd & bootstrap_agent & kickstart_agent

  set sh to "set -e;" & sh1 & sh2 & sh3 & sh4 & resolve_uid & unload_agent & load_service & load_agent

  do shell script sh with prompt "RustDesk wants to install daemon and agent" with administrator privileges
end run
