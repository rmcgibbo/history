use anyhow::{anyhow, Context, Result};

use crate::util::addr_routes_to_me;

/// show text that should be sourced into the bash shell with eval "$(history --eval)"
pub async fn show_bash_eval_string(server_addr: String) -> Result<()> {
    let current_exe = std::env::current_exe()
        .context("Unable to get current executable name")?
        .into_os_string()
        .into_string()
        .map_err(|_| anyhow!("Unable to format current executable name as a UTF-8 string"))?;

    let runserver = format!(
        "__history_mode=\"server\" {} --daemonize $HOME/.histdb.db",
        current_exe
    );
    if addr_routes_to_me(&server_addr).await? {
        println!("{}", runserver);
    }

    // This is a bit fiddly, so here's an explanation of what we're trying to do:
    //   1. Above, if the user runs eval "$(history --eval mymachine.foo.bar.com)", and we happen
    //      to be running on mymachine.foo.bar.com, we'll start up the server process. Note
    //      that we use this hidden environment variable to decide what the binary acts like,
    //      so that the "normal" call to ``$ history`` works like a query, and you don't see a
    //      confusing message related to there being a server mode.
    //   2. We're going to stash the location of the server in an environment variable, so that
    //      future calls to ``$ history`` (the query interface) automatically know who to talk to if
    //      executed after this stuff is eval'd into the shell.
    //    4. The PROMPT_COMMAND is a command that gets executed after a command finished and when
    //      displaying the next prompt. We set it to call our function. Note that we need to be pretty
    //      careful to deal with the case where the user already has a PROMPT_COMMAND (put ours at the
    //      end) and when we're called multiple times (only puty it in once)
    //    5. The command that we actually run is supposed to forward the information (exit code, pwd,
    //      and last executed command from the history builtin) to the server. Bash has this insane
    //      feature where you can use the pseudo-file /dev/udp/host/port to send UDP messages right
    //      from the shell (https://tightlycoupled.io/send-udp-messages-with-dev-udp/). That's nice
    //      because it means we don't need to invoke an extra process from within the callback, which
    //      is always a risk (what if the process is slow, hangs, crashes). In particular if you invoke
    //      a process from PROMPT_COMMAND and it hangs, now your shell is hung.
    //

    // Note: I've been through a few different prior versions of the design here.
    //
    // 1. I had a prior design where I wrote another mode for this binary that just sent the UDP message
    //    to the server, and then invoked that from the PROMPT_COMMAND. I didn't like that because I
    //    figured it was inefficient to start up a new process, and wanted to see if I could do better.
    // 2. I had a prior version where I started up a "UDP forwarder" process in the background that read
    //    from stdin and then sent it along to the server, and then the PROMPT_COMMAND just did a
    //    printf ... > pipe. The problem with this is that if the UDP forwarder gets killed for any reason,
    //    the printf hangs since writes to a pipe are blocking.
    // 3. I had a version where I used a bash coprocess. Here are the notes for that:
    //     a) Then, we start up a "bash coprocess", which is a background process that is listening
    //        on stdin and forwards information it receives over UDP to the server process. It knows
    //        the identity of the server from the environment variable. Bash sets this up so that
    //        the "__history_forwarder_PID" variable will contain the PID of the coprocess, and
    //        '__history_forwarder' is an array containing the read and write file descriptors.
    //        See e.g. https://copyconstruct.medium.com/bash-coprocess-2092a93ad912
    //     b) The risk here, and what makes this all tricky, is that writing to a pipe is blocking. If
    //        the coprocess doesn't exist or is stuck, then the promt command take a long time and that
    //        literally hangs the user's shell. Bash sort of has our back here, and this is why it's better
    //        to use a coprocess than just a normal background process: The __history_forwarder and
    //        __history_forwarder_PID variables are special and literally disappear within bash when the
    //        co-process exits. So if someone sigkills the coprocess, then the PID will no longer resolve
    //        nothing happens because of the guard. In the TOCTOU condition in which the PID exists when
    //        checked but then the file descriptor variable doesn't exist, you'll just get a " Bad file
    //        descriptor" warniong in the shell since you're redirection to the empty string.
    //    c)  So the real deadlock risk is if the coprocess continues to exist but hangs. Hopefully that
    //        doesn't happen. And frankly that's the same problem that exists in the alternative design where
    //        you start a process up from within __history to make the UDP RPC itself.
    //
    //    The ultimate reason I dropped the bash coprocess, beyond it being a little insane, is that when
    //    you have a coprocess running and try to exit the shell, you see:
    //
    //        [mcgibbon@pn50:~/projects/history]$ coproc sleep 60
    //        [1] 2186773
    //
    //        [mcgibbon@pn50:~/projects/history]$ exit
    //        logout
    //        There are running jobs.
    //        [1]+  Running                 coproc COPROC sleep 60 &
    //
    //   So the coprocess is running in the bash jobs table like a background job, which is going to be
    //   obvious and annoying to users trying to exit the shell or do ``kill %`` or whatever.
    //   And furthermore, if you try to work around this by ``disown``ing the coprocess, then it doesn't
    //   work properly. For example, ``kill -9``-ing a disowned coprocess causes the whole bash process
    //   to freaking die. And also the coproc-specific env variables don't actually update properly when it
    //   dies.

    let cmd = r#"export __history_server="@history_ADDR@"
__history_session=$(tty); __history_session="${__history_session/\/dev\/pts\//}"
__history() {
    local EXIT="$?"
    printf "%s\0%s\0%s\0%s\0%s" "$__history_session" "@history_HOSTNAME@" "$EXIT" "$(pwd)" "$(command history 1)" > /dev/udp/@history_ADDR@/@HISTORY_PORT@
}

unset -f __history_interactive
__history_interactive() {
    local output
    local code
    IFS=" " read -d '' -r code output < <(__history_mode=isearch __history_pwd=$(pwd) @history_EXE@ 3>&1 1>&2 2>&3)

    # If code="n", then we auto-exec the command as if the user had typed
    # enter, which is what the real ctr-r does
    if [[ "$code" == "n" ]]; then
        # 1. render the prompt
        # 2. run the command
        # 3. add it to the history buffer with command -s, so up-arrow works
        # 4. add it to _our_ history buffer by calling PROMPT_COMMAND
        printf "%s%s\n" "${PS1@P}" "$output"

        stty sane;
        eval "$output"
        command history -s "$output"
        eval "$PROMPT_COMMAND"
        [[ "$PS1" == "\n"* ]] && printf "\n"
        READLINE_LINE=""
        READLINE_POINT=0
    elif [[ "$code" == "a" ]]; then
        READLINE_LINE=${output}
        READLINE_POINT=0
    else
        READLINE_LINE=${output}
        READLINE_POINT=${#output}
    fi
}
[[ ${SHELLOPTS} =~ (vi|emacs) ]] &&  bind -x '"\C-r": __history_interactive'

if [[ -z "$PROMPT_COMMAND" ]]; then
    PROMPT_COMMAND="__history"
elif [[ "$PROMPT_COMMAND" != *"__history"* ]]; then
    PROMPT_COMMAND="$PROMPT_COMMAND; __history";
fi

history() {
    if [[ $(command caller) == *"/etc/bashrc" ]]; then
        command history "$@"
    else
    __history_pwd=$(pwd) @history_EXE@ "$@"
    fi
}
"#;

    println!(
        "{}",
        cmd.replace("@history_EXE@", &current_exe)
            .replace("@history_ADDR@", &server_addr)
            .replace("@history_HOSTNAME@", &*crate::MYHOSTNAME,)
            .replace("@HISTORY_PORT@", &format!("{}", crate::HISTORY_PORT))
    );

    Ok(())
}
