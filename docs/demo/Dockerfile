# The recording environment for the README demos.
#
# Linux rather than the maintainer's Mac: gwt prints absolute paths, and
# `/home/me/worktrees/feature/auth` reads like the README while macOS would put
# `/private/tmp/...` or a real user name on screen. It also keeps the font, the
# theme and the shell identical whoever re-records.
FROM ghcr.io/charmbracelet/vhs:latest

# vhs ships without git, which is the one thing gwt cannot do without.
RUN apt-get update \
    && apt-get install -y --no-install-recommends git \
    && rm -rf /var/lib/apt/lists/*

ENV HOME=/home/me
RUN mkdir -p /home/me
WORKDIR /home/me
