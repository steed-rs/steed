# PLEASE NOTE: This is very much a WIP, don't expect it to work good, and it will definitely break every once in a while.

# Steed
> Steed is your trusty ride into Battle.net!


Steed is a set of libraries and a CLI tools providing implementations of various NGDP related technologies. It's written in Rust 🦀.

## Current state of affairs
As of writing (November 22, 2022) the libraries and CLI tools can currently do the following:
- Perform a full install of WoW, downloading files from CDN and building CASC archives
    - Well, almost, there's a few files missing that get streamed in, likely due to me not using the correct list of tags when filtering the install manifest
    - Also supports some level of restartability in case of errors, although it doesn't exit nicely on Ctrl-C
- Read files from a local CASC archive
    - No actual CLI tool, but the support is all ther
- Full BLTE encode and decode support
    - Tested against all files in the latest build of WoW, with known encryption keys, or without encryption

## How to use what's here
First of all you need an up-to-date rust toolchain installed.

Create a file named `config.toml` in the root of the repo, with the following keys:
```
# A directory containing a local WoW install. Not actually used by the "install" subcommand
wow_path = "/path/to//World of Warcraft/"

# A directory containing a checkout of https://github.com/wowdev/TACTKeys
tactkeys_path = "/path/to/TACTKeys/"

# Path to a listfile. For example https://github.com/wowdev/wow-listfile. Not actually used by the "install" subcommand
listfile_path = "/path/to/wow-listfile/community-listfile.csv"

# If you want to target a specific CDN or a local mirror, you can specify that here
# cdn_override = "http://localhost:8080/"
```

Then run one of the following commands:
- To install WoW to a local directory: `cargo run --release --bin steed-cli install /path/to/install/wow`
- To download Battle.net catalogs and write them to stdout: `cargo run --release --bin steed-cli catalog`
- To run whatever self-test that was last commited: `cargo run --release --bin steed-cli`

***NOTE:***:
- I haven't tested it for a little while so it might have broken with pre-patch
- This was developed and tested on Linux, so it might make assumptions that don't hold on Windows/MacOS

## Future plans
The following is a list of things I'd like to improve/add in the short term:
- Clean up the install part of CLI:
    - Support different regions
    - Better error tolerance. Currently exits on error, but restarts where it left off
    - Figure out a format (preferabbly the one the Battle.net client uses) to store version information about the installed game
- Add update support to the CLI:
    - BLTE encode already in place
    - The following is unimplemented:
        - Patch manifest parsing
        - Bindiff implementation to actually apply patches to decoded files

In the long term, some plans are as follow:
- Agent like service + API mode that can be used by a graphical client
- Support for installing non-WoW games
    - Already have the code to parse Battle.net catalogs
- Nicer tooling around reading local CASC archives
- C FFI interface for integration with other language