# What is hsh?
hsh is a shell and can be used instead of bash, zsh and other shells.

# Setting as default shell on HackerOS
```bash
# First add hsh to /etc/shells
echo "$(which hsh)" | sudo tee -a /etc/shells

# Then change the shell for the current user
chsh -s "$(which hsh)"
```
Log out and log in again and you're done.
