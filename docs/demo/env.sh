# Shell setup for the recordings. Sourced by the tapes while vhs is hiding,
# so none of this ends up on screen.

bash /demo/setup.sh

# `\w` is what makes the demos worth watching: `gwx cd` and the picker move the
# shell, and the prompt is where you see it happen.
export PS1='\[\e[1;36m\]\w\[\e[0m\] $ '

eval "$(gwx shell-init bash)"

cd ~/repo
clear
