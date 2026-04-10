# TODO list

This is a list of current tasks for sketch. Any contribution is welcome!

### Todo

- [ ] Add `--as-root` argument. Run sketch session as user by default even when executed with `sudo`
- [ ] Figure out best testing approach (`cargo test` does not play well with `sudo`)
- [ ] Add commit functionality to directories (`-r` argument for recursive)
- [ ] Plan sudoless access (need to figure out best approach)
- [ ] Setup github workflows for testing
- [ ] Update metadata (need to include all overlays)
- [ ] Support `attach` command to restore disconnected session
- [ ] Change bash interactive session to login session
- [ ] Review name config variable
- [ ] Add option to preserve current dir

### Done ✓

- [x] Commit functionality (`sketch commit file.txt` withing an active session)
- [x] Add `--x11` option to bind `/tmp/.X11-unix`
