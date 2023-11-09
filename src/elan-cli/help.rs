pub static ELAN_HELP: &'static str = r"DISCUSSION:
    elan manages your installations of the Lean theorem prover.
    It places `lean` and `lake` binaries in your `PATH` that automatically
    select and, if necessary, download the Lean version described in your
    project's `lean-toolchain` file. You can also install, select, run,
    and uninstall Lean versions manually using the commands of the `elan`
    executable.";

pub static SHOW_HELP: &'static str = r"DISCUSSION:
    Shows the name of the active toolchain and the version of `lean`.

    If there are multiple toolchains installed then all installed
    toolchains are listed as well.";

pub static UPDATE_HELP: &'static str = r"DISCUSSION:
    With no toolchain specified, the `update` command updates each of
    the installed toolchains from the official release channels, then
    updates elan itself.

    If given a toolchain argument then `update` updates that
    toolchain, the same as `elan toolchain install`.";

pub static INSTALL_HELP: &'static str = r"DISCUSSION:
    Installs a specific lean toolchain.

    The 'install' command is an alias for 'elan update <toolchain>'.";

pub static DEFAULT_HELP: &'static str = r"DISCUSSION:
    Sets the default toolchain to the one specified. If the toolchain
    is not already installed then it is installed first.";

pub static TOOLCHAIN_HELP: &'static str = r"DISCUSSION:
    Many `elan` commands deal with *toolchains*, a single
    installation of the Lean theorem prover. `elan` supports multiple
    types of toolchains. The most basic track the official release
    channels: 'stable', 'beta', and 'nightly'; but `elan` can also
    install toolchains from the official archives and from local builds.

    Standard release channel toolchain names have the following form:

        [<origin>:]<channel>[-<date>]

        <channel>       = stable|beta|nightly|<version>
        <date>          = YYYY-MM-DD

    'channel' is either a named release channel or an explicit version
    number, such as '4.0.0'. Channel names can be optionally appended
    with an archive date, as in 'nightly-2023-06-27', in which case
    the toolchain is downloaded from the archive for that date.
    'origin' can be used to refer to custom forks of Lean on Github;
    the default is 'leanprover/lean4'. For nightly versions, '-nightly'
    is appended to the value of 'origin'.

    elan can also manage symlinked local toolchain builds, which are
    often used to for developing Lean itself. For more information see
    `elan toolchain help link`.";

pub static TOOLCHAIN_LINK_HELP: &'static str = r"DISCUSSION:
    'toolchain' is the custom name to be assigned to the new toolchain.

    'path' specifies the directory where the binaries and libraries for
    the custom toolchain can be found. For example, when used for
    development of Lean itself, toolchains can be linked directly out of
    the Lean root directory. After building, you can test out different
    compiler versions as follows:

        $ elan toolchain link master <path/to/lean/root>
        $ elan override set master

    If you now compile a crate in the current directory, the custom
    toolchain 'master' will be used.";

pub static OVERRIDE_HELP: &'static str = r"DISCUSSION:
    Overrides configure elan to use a specific toolchain when
    running in a specific directory.

    elan will automatically select the Lean toolchain specified in
    the `lean-toolchain` file when inside a Lean package, but
    directories can also be assigned their own Lean toolchain manually
    with `elan override`. When a directory has an override then any
    time `lean` or `lake` is run inside that directory, or one of
    its child directories, the override toolchain will be invoked.

    To pin to a specific nightly:

        $ elan override set nightly-2023-09-06

    Or a specific stable release:

        $ elan override set 4.0.0

    To see the active toolchain use `elan show`. To remove the
    override and use the default toolchain again, `elan override
    unset`.";

pub static OVERRIDE_UNSET_HELP: &'static str = r"DISCUSSION:
    If `--path` argument is present, removes the override toolchain
    for the specified directory. If `--nonexistent` argument is
    present, removes the override toolchain for all nonexistent
    directories. Otherwise, removes the override toolchain for the
    current directory.";

pub static RUN_HELP: &'static str = r"DISCUSSION:
    Configures an environment to use the given toolchain and then runs
    the specified program. The command may be any program, not just
    lean or lake. This can be used for testing arbitrary toolchains
    without setting an override.

    Commands explicitly proxied by `elan` (such as `lean` and
    `lake`) also have a shorthand for this available. The toolchain
    can be set by using `+toolchain` as the first argument. These are
    equivalent:

        $ lake +nightly build

        $ elan run nightly lake build";

pub static _DOC_HELP: &'static str = r"DISCUSSION:
    Opens the documentation for the currently active toolchain with
    the default browser.

    By default, it opens the documentation index. Use the various
    flags to open specific pieces of documentation.";

pub static COMPLETIONS_HELP: &'static str = r"DISCUSSION:
    One can generate a completion script for `elan` that is
    compatible with a given shell. The script is output on `stdout`
    allowing one to re-direct the output to the file of their
    choosing. Where you place the file will depend on which shell, and
    which operating system you are using. Your particular
    configuration may also determine where these scripts need to be
    placed.

    Here are some common set ups for the three supported shells under
    Unix and similar operating systems (such as GNU/Linux).

    BASH:

    Completion files are commonly stored in `/etc/bash_completion.d/`.
    Run the command:

        $ elan completions bash > /etc/bash_completion.d/elan.bash-completion

    This installs the completion script. You may have to log out and
    log back in to your shell session for the changes to take affect.

    BASH (macOS/Homebrew):

    Homebrew stores bash completion files within the Homebrew directory.
    With the `bash-completion` brew formula installed, run the command:

        $ elan completions bash > $(brew --prefix)/etc/bash_completion.d/elan.bash-completion

    FISH:

    Fish completion files are commonly stored in
    `$HOME/.config/fish/completions`. Run the command:

        $ elan completions fish > ~/.config/fish/completions/elan.fish

    This installs the completion script. You may have to log out and
    log back in to your shell session for the changes to take affect.

    ZSH:

    ZSH completions are commonly stored in any directory listed in
    your `$fpath` variable. To use these completions, you must either
    add the generated script to one of those directories, or add your
    own to this list.

    Adding a custom directory is often the safest bet if you are
    unsure of which directory to use. First create the directory; for
    this example we'll create a hidden directory inside our `$HOME`
    directory:

        $ mkdir ~/.zfunc

    Then add the following lines to your `.zshrc` just before
    `compinit`:

        fpath+=~/.zfunc

    Now you can install the completions script using the following
    command:

        $ elan completions zsh > ~/.zfunc/_elan

    You must then either log out and log back in, or simply run

        $ exec zsh

    for the new completions to take affect.

    CUSTOM LOCATIONS:

    Alternatively, you could save these files to the place of your
    choosing, such as a custom directory inside your $HOME. Doing so
    will require you to add the proper directives, such as `source`ing
    inside your login script. Consult your shells documentation for
    how to add such directives.

    POWERSHELL:

    The powershell completion scripts require PowerShell v5.0+ (which
    comes Windows 10, but can be downloaded separately for windows 7
    or 8.1).

    First, check if a profile has already been set

        PS C:\> Test-Path $profile

    If the above command returns `False` run the following

        PS C:\> New-Item -path $profile -type file -force

    Now open the file provided by `$profile` (if you used the
    `New-Item` command it will be
    `%USERPROFILE%\Documents\WindowsPowerShell\Microsoft.PowerShell_profile.ps1`

    Next, we either save the completions file into our profile, or
    into a separate file and source it inside our profile. To save the
    completions into our profile simply use

        PS C:\> elan completions powershell >> %USERPROFILE%\Documents\WindowsPowerShell\Microsoft.PowerShell_profile.ps1";

pub static TOOLCHAIN_ARG_HELP: &'static str = "Toolchain name, such as 'stable', 'beta', nightly', \
     or '4.0.0'. For more information see `elan \
     help toolchain`";
