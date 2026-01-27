export SSHPASS
SSHPASS="$(gpg -d -q ~/.sshpasswd.gpg | tr -d '\n')"