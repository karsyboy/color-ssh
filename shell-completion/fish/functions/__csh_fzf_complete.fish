# Recursively parse an SSH config file and all Include directives.
# This flattens the full SSH config into a single stream of lines,
# preserving order and relative include paths.
function __csh_parse_config --argument-names file
    # Exit early if the config file does not exist
    if not test -f "$file"
        return
    end

    # Base directory used to resolve relative Include paths
    set base (dirname "$file")

    # Read the file line-by-line (including final line without newline)
    while read -l line
        # Detect Include directives (case-insensitive)
        if string match -ri '^include\s+' -- $line
            # Extract paths after "Include"
            set paths (string replace -ri '^include\s+' '' -- $line)

            # Handle multiple include paths on one line
            for p in $paths
                # Expand ~ and environment variables
                set p (eval echo $p)

                # Resolve relative paths relative to current config file
                if not string match -q '/*' -- $p
                    set p "$base/$p"
                end

                # Expand globs and recursively parse included files
                for f in (ls $p 2>/dev/null)
                    __csh_parse_config $f
                end
            end
        else
            # Emit non-Include lines unchanged
            echo $line
        end
    end < $file
end


# Build a structured host table from the SSH config.
# Output format (pipe-delimited):
#   alias|hostname|user|description
function __csh_host_table
    set config "$HOME/.ssh/config"

    __csh_parse_config $config \
    | awk '
        BEGIN {
            IGNORECASE=1
            host=""
            hostname=""
            user=""
            desc=""
        }

        # When a new Host block starts, flush the previous one
        $1=="Host" {
            if (host && hostname && host !~ /\*/) {
                printf "%s|%s|%s|%s\n", host, hostname, user, desc
            }

            # Reset state for the new Host block
            host=""
            hostname=""
            user=""
            desc=""

            # Capture the first non-wildcard alias
            for (i=2;i<=NF;i++) {
                if ($i !~ /\*/) host=$i
            }
        }

        # Capture commonly-used SSH directives
        $1=="HostName" { hostname=$2 }
        $1=="User"     { user=$2 }

        # Custom description comment (#_desc ...)
        $1=="#_desc"   { desc=substr($0, index($0,$2)) }

        # Flush the final Host block
        END {
            if (host && hostname && host !~ /\*/) {
                printf "%s|%s|%s|%s\n", host, hostname, user, desc
            }
        }
    ' | sort -u
end


# Launch fzf to interactively select a host.
# Displays a columnized table with a live SSH config preview.
function __csh_fzf_select --argument-names query
    # Table header displayed above results
    set header "Alias|Hostname|User|Desc"
    set sep    "─────|────────|────|────"

    # Generate host table
    set data (__csh_host_table)
    if test -z "$data"
        return
    end

    # Run fzf and capture both key + selection
    set result (
        printf "%s\n%s\n%s\n" $header $sep $data \
        | column -t -s '|' \
        | fzf \
            --ansi \
            --height 40% \
            --border \
            --cycle \
            --reverse \
            --info=inline \
            --header-lines=2 \
            --prompt 'CSH Remote > ' \
            --query "$query" \
            --no-separator \
            # Tab navigation + backspace-to-exit behavior
            --bind 'shift-tab:up,tab:down,bspace:backward-delete-char/eof' \
            # Preview the fully-resolved SSH config using ssh as source of truth
            --preview '
                        ssh -G {1} 2>/dev/null \
                        | grep -i -E "^(user|hostname|port|controlmaster|forwardagent|localforward|identityfile|remoteforward|proxycommand|proxyjump)[[:space:]]" \
                        | awk "{ \$1 = toupper(substr(\$1,1,1)) substr(\$1,2); print }" \
                        | column -t
                        ' \
            --preview-window=right:40% \
            --expect=alt-enter,enter
    )

    # Exit if fzf was cancelled
    if test -z "$result"
        return
    end

    # First line = pressed key, last line = selected row
    set key (string split \n $result)[1]
    set row (string split \n $result)[-1]

    # Extract alias (first column)
    set host (string split ' ' $row)[1]

    # Return key + host to caller
    echo "$key|$host"
end


# Fish completion entrypoint.
# Replaces the current commandline with `csh <host>` and optionally executes it.
function __csh_fzf_complete
    # Current token under cursor (used as fzf query)
    set query (commandline -ct)

    set res (__csh_fzf_select "$query")
    if test -z "$res"
        return
    end

    # Split result into key + host
    set key (string split '|' $res)[1]
    set host (string split '|' $res)[2]

    # Replace commandline with selected host
    commandline -r "csh $host"

    # Execute immediately unless Alt-Enter was used
    if test "$key" = "enter"
        commandline -f execute
    end
end