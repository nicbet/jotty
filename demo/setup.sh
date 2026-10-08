# Sourced by demo.tape: a throwaway project and jot store, so the recording
# never touches your real jots in ~/.config/jotty.
export XDG_CONFIG_HOME=/tmp/jotty-demo/config
export PATH="$PWD/target/release:$PATH"
rm -rf /tmp/jotty-demo
mkdir -p /tmp/jotty-demo/webapp/.git "$XDG_CONFIG_HOME/jotty"
cd /tmp/jotty-demo/webapp || return

cat > "$XDG_CONFIG_HOME/jotty/config.toml" <<'TOML'
[[highlight]]
pattern = '\b[a-z]+-[0-9a-f]{6}\b'
TOML

cat > "$(jotty path)" <<JOTS
<!-- jotty: $(pwd -P) -->

<!-- $(date -v-3d '+%Y-%m-%d') 09:12 -->
## Ideas for the settings page
- group options by **account**, **appearance** and **privacy**
- add a search box once there are more than ~20 options
<!-- end -->

<!-- $(date -v-1d '+%Y-%m-%d') 16:40 -->
Session cookie is missing \`SameSite\` (web-4f9a2c), check with the auth middleware
<!-- end -->

<!-- $(date -v-1H '+%Y-%m-%d %H:%M') -->
## Release checklist
1. bump the version
2. update the changelog
3. tag and push
<!-- end -->
JOTS
clear
