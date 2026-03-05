#!/usr/bin/env zsh
#compdef cossh

# Zsh completion for cossh.
# Host completions are sourced from:
#   cossh __complete hosts --protocol <all|ssh|rdp>

: ${COSSH_COMPLETION_BIN:=cossh}

_cossh_completion_hosts() {
  local protocol="${1:-all}"
  "$COSSH_COMPLETION_BIN" __complete hosts --protocol "$protocol" 2>/dev/null
}

_cossh_vault_entries() {
  "$COSSH_COMPLETION_BIN" vault list 2>/dev/null
}

_cossh_profiles() {
  local config_dir="$HOME/.color-ssh"
  local -a profiles
  local file profile

  [[ -f "$config_dir/cossh-config.yaml" ]] && profiles+=("default")

  for file in "$config_dir"/*.cossh-config.yaml(N); do
    profile="${file:t}"
    profile="${profile%.cossh-config.yaml}"
    [[ -n "$profile" ]] && profiles+=("$profile")
  done

  printf '%s\n' ${(ou)profiles}
}

_cossh() {
  local cur prev state
  local subcmd=""
  local subcmd_index=0
  local idx=2

  cur="${words[CURRENT]}"
  prev="${words[CURRENT-1]}"

  while (( idx <= $#words )); do
    case "${words[idx]}" in
      -P|--profile|--pass-entry)
        (( idx += 2 ))
        continue
        ;;
      --profile=*|--pass-entry=*)
        (( idx += 1 ))
        continue
        ;;
      -d|-l|-t|--debug|--log|--test|--migrate)
        (( idx += 1 ))
        continue
        ;;
      ssh|rdp|vault)
        subcmd="${words[idx]}"
        subcmd_index=$idx
        break
        ;;
      *)
        break
        ;;
    esac
  done

  if [[ -z "$subcmd" ]]; then
    _arguments -C \
      '(-d --debug)'{-d,--debug}'[Enable debug logging to ~/.color-ssh/logs/cossh.log; repeat (-dd) for raw terminal and argument tracing]' \
      '(-l --log)'{-l,--log}'[Enable SSH session logging to ~/.color-ssh/logs/ssh_sessions/]' \
      '(-P --profile)'{-P+,--profile=}'[Specify a configuration profile to use]:profile name:->profile' \
      '(-t --test)'{-t,--test}'[Ignore config logging settings; only use CLI -d/-l logging flags]' \
      '--pass-entry=[Override the password vault entry used for a direct protocol launch]:vault entry:->pass_entry' \
      '--migrate[Migrate ~/.ssh/config host entries into ~/.color-ssh/cossh-inventory.yaml]' \
      '1:subcommand:->subcommand'

    case "$state" in
      profile)
        _wanted profiles expl 'profile' compadd -- "${(@f)$(_cossh_profiles)}"
        return
        ;;
      pass_entry)
        _wanted entries expl 'vault entry' compadd -- "${(@f)$(_cossh_vault_entries)}"
        return
        ;;
      subcommand)
        _values 'subcommand' \
          'ssh[Launch an SSH session by forwarding arguments to the SSH command]' \
          'rdp[Launch an RDP session using xfreerdp3 or xfreerdp]' \
          'vault[Manage the password vault]'
        return
        ;;
    esac

    return
  fi

  case "$subcmd" in
    ssh)
      if [[ "$cur" != -* ]]; then
        _wanted hosts expl 'SSH inventory host' compadd -- "${(@f)$(_cossh_completion_hosts ssh)}"
      fi
      ;;
    rdp)
      case "$prev" in
        -u|--user)
          _message 'RDP username'
          return
          ;;
        -D|--domain)
          _message 'RDP domain'
          return
          ;;
        -p|--port)
          _message 'RDP port'
          return
          ;;
      esac

      if [[ "$cur" == -* ]]; then
        _values 'RDP option' \
          '-u[Override the RDP username]' \
          '--user[Override the RDP username]' \
          '-D[Override the RDP domain]' \
          '--domain[Override the RDP domain]' \
          '-p[Override the RDP port]' \
          '--port[Override the RDP port]'
      else
        _wanted hosts expl 'RDP inventory host' compadd -- "${(@f)$(_cossh_completion_hosts rdp)}"
      fi
      ;;
    vault)
      local vault_action="${words[subcmd_index+1]}"

      if (( CURRENT == subcmd_index + 1 )); then
        _values 'vault subcommand' \
          'init[Initialize the password vault]' \
          'add[Create or replace a password vault entry interactively]' \
          'remove[Remove a password vault entry]' \
          'list[List password vault entries]' \
          'unlock[Unlock the shared password vault]' \
          'lock[Lock the shared password vault]' \
          'status[Show shared password vault status]' \
          'set-master-password[Create or rotate the password vault master password]'
        return
      fi

      case "$vault_action" in
        remove)
          _wanted entries expl 'vault entry' compadd -- "${(@f)$(_cossh_vault_entries)}"
          ;;
        add)
          _message 'vault entry name'
          ;;
      esac
      ;;
  esac
}

compdef _cossh cossh
