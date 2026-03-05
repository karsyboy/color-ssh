# Fish completion definitions for cossh.
#
# Keep helper functions in this file so they are guaranteed to be loaded
# before completion conditions (`-n`) evaluate.

if not functions -q __cossh_completion_bin
    function __cossh_completion_bin
        if set -q COSSH_COMPLETION_BIN
            echo $COSSH_COMPLETION_BIN
        else
            echo cossh
        end
    end

    function __cossh_inventory_hosts --argument-names protocol
        if test -z "$protocol"
            set protocol all
        end

        set -l bin (__cossh_completion_bin)
        $bin __complete hosts --protocol $protocol 2>/dev/null
    end

    function __cossh_ssh_hosts
        __cossh_inventory_hosts ssh
    end

    function __cossh_rdp_hosts
        __cossh_inventory_hosts rdp
    end

    function __cossh_vault_entries
        set -l bin (__cossh_completion_bin)
        $bin vault list 2>/dev/null
    end

    function __cossh_profiles
        set -l config_dir "$HOME/.color-ssh"
        test -d "$config_dir"; or return

        set -l profiles

        if test -f "$config_dir/cossh-config.yaml"
            set profiles $profiles default
        end

        for path in (command ls -1 "$config_dir"/*.cossh-config.yaml 2>/dev/null)
            set -l file (basename "$path")
            set -l profile (string replace -r '\.cossh-config\.yaml$' '' -- "$file")
            if test -n "$profile"
                set profiles $profiles $profile
            end
        end

        if test (count $profiles) -gt 0
            printf "%s\n" $profiles | sort -u
        end
    end

    function __cossh_subcommand_name
        set -l tokens (commandline -opc)
        set -l expect_value 0

        for i in (seq (count $tokens))
            set -l token "$tokens[$i]"
            if test $i -eq 1
                continue
            end

            if test $expect_value -eq 1
                set expect_value 0
                continue
            end

            switch "$token"
                case '-P' '--profile' '--pass-entry'
                    set expect_value 1
                case '--profile=*' '--pass-entry=*'
                case 'ssh' 'rdp' 'vault'
                    echo "$token"
                    return 0
                case '-*'
                case '*'
                    return 1
            end
        end

        return 1
    end

    function __cossh_use_subcommand
        set -l subcommand (__cossh_subcommand_name)
        if test -z "$subcommand"
            return 0
        end
        return 1
    end

    function __cossh_seen_subcommand --argument-names expected
        set -l subcommand (__cossh_subcommand_name)
        if test "$subcommand" = "$expected"
            return 0
        end
        return 1
    end

    function __cossh_current_token_not_option
        set -l token (commandline -ct)
        if string match -qr '^-' -- $token
            return 1
        end
        return 0
    end

    function __cossh_need_vault_action
        __cossh_seen_subcommand vault; or return 1
        set -l tokens (commandline -opc)
        set -l expect_value 0
        set -l seen_vault 0

        for i in (seq (count $tokens))
            set -l token "$tokens[$i]"
            if test $i -eq 1
                continue
            end

            if test $seen_vault -eq 1
                switch "$token"
                    case init add remove list unlock lock status set-master-password
                        return 1
                    case '*'
                        return 0
                end
            end

            if test $expect_value -eq 1
                set expect_value 0
                continue
            end

            switch "$token"
                case '-P' '--profile' '--pass-entry'
                    set expect_value 1
                case '--profile=*' '--pass-entry=*'
                case vault
                    set seen_vault 1
                case 'ssh' 'rdp'
                    return 1
                case '-*'
                case '*'
                    return 1
            end
        end

        return 1
    end

    function __cossh_vault_action --argument-names action
        __cossh_seen_subcommand vault; or return 1
        set -l tokens (commandline -opc)
        set -l expect_value 0
        set -l seen_vault 0

        for i in (seq (count $tokens))
            set -l token "$tokens[$i]"
            if test $i -eq 1
                continue
            end

            if test $seen_vault -eq 1
                if test "$token" = "$action"
                    return 0
                end
                return 1
            end

            if test $expect_value -eq 1
                set expect_value 0
                continue
            end

            switch "$token"
                case '-P' '--profile' '--pass-entry'
                    set expect_value 1
                case '--profile=*' '--pass-entry=*'
                case vault
                    set seen_vault 1
                case 'ssh' 'rdp'
                    return 1
                case '-*'
                case '*'
                    return 1
            end
        end

        return 1
    end
end

set -l __cossh_no_subcommand "__cossh_use_subcommand"

# Reset existing completion rules so stale definitions from older versions do
# not remain after re-sourcing this file.
complete -e -c cossh 2>/dev/null

# Disable default file/path completion fallback for this command.
complete -c cossh -f

# Top-level options and subcommands.
complete -c cossh -n "$__cossh_no_subcommand" -s d -l debug -d "Enable debug logging (-dd for raw terminal and argument tracing)"
complete -c cossh -n "$__cossh_no_subcommand" -s l -l log -d "Enable SSH session logging"
complete -c cossh -n "$__cossh_no_subcommand" -s P -l profile -x -a "(__cossh_profiles)" -d "Specify a configuration profile"
complete -c cossh -n "$__cossh_no_subcommand" -s t -l test -d "Ignore config logging settings; only use CLI -d/-l logging flags"
complete -c cossh -n "$__cossh_no_subcommand" -l pass-entry -r -xa "(__cossh_vault_entries)" -d "Override the direct-launch password vault entry"
complete -c cossh -n "$__cossh_no_subcommand" -l migrate -d "Migrate ~/.ssh/config host entries into ~/.color-ssh/cossh-inventory.yaml"
complete -c cossh -n "$__cossh_no_subcommand" -a "ssh rdp vault"

# `cossh ssh` host completions.
complete -c cossh -n "__cossh_seen_subcommand ssh; and __cossh_current_token_not_option" -a "(__cossh_ssh_hosts)" -d "SSH inventory host"

# `cossh rdp` options and host completions.
complete -c cossh -n "__cossh_seen_subcommand rdp" -s u -l user -r -d "Override the RDP username"
complete -c cossh -n "__cossh_seen_subcommand rdp" -s D -l domain -r -d "Override the RDP domain"
complete -c cossh -n "__cossh_seen_subcommand rdp" -s p -l port -r -d "Override the RDP port"
complete -c cossh -n "__cossh_seen_subcommand rdp; and __cossh_current_token_not_option" -a "(__cossh_rdp_hosts)" -d "RDP inventory host"

# `cossh vault` action and argument completions.
complete -c cossh -n "__cossh_need_vault_action" -a "init add remove list unlock lock status set-master-password"
complete -c cossh -n "__cossh_vault_action remove" -a "(__cossh_vault_entries)" -d "Vault entry"
complete -c cossh -n "__cossh_vault_action add" -f -d "Vault entry name"
