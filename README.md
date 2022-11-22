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