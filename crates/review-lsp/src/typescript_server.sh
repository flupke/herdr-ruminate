# The project's direnv or Nix environment may provide no command but the shell's built-ins.

local_bin() {
    dir=$PWD
    while :; do
        if [ -x "$dir/node_modules/.bin/$1" ]; then
            printf '%s\n' "$dir/node_modules/.bin/$1"
            return 0
        fi
        [ -z "$dir" ] && return 1
        dir=${dir%/*}
    done
}

# TypeScript 7 and later is the native compiler, whose tsc serves LSP.
native_tsc() {
    case $("$1" --version </dev/null 2>/dev/null) in
        "Version "[7-9].* | "Version "[1-9][0-9]*) return 0 ;;
        *) return 1 ;;
    esac
}

path_bin() {
    command -v "$1" 2>/dev/null
}

for find in local_bin path_bin; do
    if bin=$($find tsgo); then exec "$bin" --lsp --stdio; fi
    if bin=$($find tsc) && native_tsc "$bin"; then exec "$bin" --lsp --stdio; fi
done
for find in local_bin path_bin; do
    if bin=$($find typescript-language-server); then exec "$bin" --stdio; fi
done
echo 'Install tsgo, TypeScript 7 or typescript-language-server in the project environment' >&2
exit 127
