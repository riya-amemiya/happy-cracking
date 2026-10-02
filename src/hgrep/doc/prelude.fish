function __!PROG!_contains_opt --description 'Specialized __fish_contains_opt'
    if not set -q __!PROG!_config
        set -g __!PROG!_config
        if set -qx RIPGREP_CONFIG_PATH
            set __!PROG!_config (
                cat -- $RIPGREP_CONFIG_PATH 2>/dev/null \
                | string trim \
                | string match -rv '^$|^#'
            )
        end
    end

    set -l commandline (commandline -cpo) (commandline -ct) $__!PROG!_config

    if contains -- "--$argv[1]" $commandline
        return 0
    end

    if set -q argv[2]
        if string match -qr -- "^-[^-]*$argv[2]" $commandline
            return 0
        end
    end

    return 1
end
