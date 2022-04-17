history
=======

This is a replacement for the `history` builtin in `bash`.

It has a couple of additional features that relative to the one included with `bash`:

1. Consolidated shell command history from multiple terminals across multiple hosts. All history is
   stored in a single sqlite database. This is done without slowing down your shell or requiring
   a shared filesystem -- additions to the history are sent over UDP to a persistent server process.
2. Replacement `history` command with enhanced search features, like temporal predicates
   (`history --since '1 day ago'`) and searches for commands you performed within a specific
   directory (`history --at .`).
3. `Ctrl-r` keybinding, which looks visually identical to one included with `bash`, but changes the
   behavior slightly to be more useful.
      - `Ctrl-r` searches the full multi-host multi-terminal database, but prefers hits from commands you
	    performed within the current working directory.
      - The search state isn't persistent in the same way as it is with the builtin, so `Ctrl-r` always
	    start searching from the same place, and after exiting out of `Ctrl-r`, the behavior of the up
		and down arrow bindings are not modified.

Usage
=====

If you're operating inside a company cluster or university computer system and you want to consolidate history
so that it's shared no matter which box you're logged into, you'll first need to pick which machine you want
to act as the "server". Your login node / workstation would be a good choice.

Then just add
```
eval "$(/path/to/binary/history --eval myworkstation.mycompany.com)"
```

to your `.bashrc` file.

If you don't care to pool your history across multiple boxes, then
you can add

Then just add
```
eval "$(/path/to/binary/history --eval 127.0.0.1)"
```

to your `.bashrc` file instead.


![image](https://user-images.githubusercontent.com/641278/163729109-5d9542a1-d2ab-4a8e-8111-81b377172ebd.png)


Inspiration / alternatives
==========================
1. [zsh-history](https://github.com/larkery/zsh-history): I copied the database schema and a lot of the CLI options from zsh-history, which
   is very nice. I don't use zsh, so that was the first difference. (The other difference is that zsh-history
   doesn't pool history from multiple machines without a shared filesystem, I think.)
2. [atuin](https://github.com/larkery/zsh-history): wow, this looks very fancy, nice job. I'm personally looking for something a bit
   more minimal with the `ctrl-r` keybindings, but this looks great.
3. [mcfly](https://github.com/cantino/mcfly): Some cool ideas here, but I was personally not sold on the use of machine learning
   for this application.
4. [bash-history-sqlite](https://github.com/thenewwazoo/bash-history-sqlite): Nice and simple. I like this.
