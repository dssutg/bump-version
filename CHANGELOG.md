# Changelog

## [2.1.1](https://github.com/dssutg/bump-version/compare/v2.1.0...v2.1.1) (2026-10-09)


### Bug Fixes

* accept colons in body prose and install a working commit-msg hook [72adcdc](https://github.com/dssutg/bump-version/commit/72adcdce82089700b300ac30743731d3e3668251)

## [2.1.0](https://github.com/dssutg/bump-version/compare/v2.0.0...v2.1.0) (2026-10-09)


### Features

* **release:** optionally bump package.json version [5ac7cab](https://github.com/dssutg/bump-version/commit/5ac7cabddb1fc6802b499ba6b1784a10851da5e2)


### Chores

* add repomap to gitignore [bdf5877](https://github.com/dssutg/bump-version/commit/bdf58778613e9bc2b85b8cb57fbdcb8d76396cce)

## [2.0.0](https://github.com/dssutg/bump-version/compare/v1.0.0...v2.0.0) (2026-10-09)


### Features

* **changelog:** [BREAKING CHANGE] grouped sections, remote compare links and credential stripping [6c192b4](https://github.com/dssutg/bump-version/commit/6c192b486d09fe0a9f33ba8c249c8c5b01edac07)
* **cli:** colored output for POSIX terminals [eb8c574](https://github.com/dssutg/bump-version/commit/eb8c574e2b4411d4d3b511e0d61390ee1001aeea)
* **config:** [BREAKING CHANGE] replace JSON with key-value config format, use modern Go features [0392805](https://github.com/dssutg/bump-version/commit/039280593e8ed79398c0b571cd278b24d165b996)
* **release:** [BREAKING CHANGE] confirmation prompt, dry-run, worktree and tag guards, annotated tags [11afc73](https://github.com/dssutg/bump-version/commit/11afc732b709d38ede79c1a0a4b2953fb8741c3e)
* **semver:** [BREAKING CHANGE] strict version parsing, prerelease tags, comparison and release types [7d78c52](https://github.com/dssutg/bump-version/commit/7d78c52fb385426572179ac615fb604e728ff7cd)


### Refactoring

* use strings.Lines() [4b5c534](https://github.com/dssutg/bump-version/commit/4b5c53487def718691c309d175518893e18658d8)


### Documentation

* add LICENSE [294997a](https://github.com/dssutg/bump-version/commit/294997a9e673dbf6c82db844f215be81d9fd9e8b)
* document the codebase and refresh stale docs [7bc0520](https://github.com/dssutg/bump-version/commit/7bc052085be502a4d2f149efa195b630cce403da)
* make docs English first [28b3aaf](https://github.com/dssutg/bump-version/commit/28b3aafa99c6e86b8c959478a2323d55bcc45eb2)
* **README.md:** remove duplicate example [0f839c7](https://github.com/dssutg/bump-version/commit/0f839c772b8f2b0836b33b2b21dc1ab6b76de0bb)
* shorten texts [974a2a3](https://github.com/dssutg/bump-version/commit/974a2a322d32481ad7da12bc20f0254cfb1aa82d)
* update email [017ed0a](https://github.com/dssutg/bump-version/commit/017ed0a57d8c49b89ac31e35b8ebce2de59a11dc)


### Continuous Integration

* create go.yml [ab0e24c](https://github.com/dssutg/bump-version/commit/ab0e24c7a1a8b461a6db57519abaca15de127599)
* fix build [e3ea3bd](https://github.com/dssutg/bump-version/commit/e3ea3bd00dcad70b70029230415423da8c0c42f0)


### Chores

* **ai:** add memory files for agents [b38d4e8](https://github.com/dssutg/bump-version/commit/b38d4e81db37d254ce4c71eefc37d00d6c80cb38)
* **ai:** delete CLAUDE.md (only standard AGENTS.md) [7328bdd](https://github.com/dssutg/bump-version/commit/7328bdd3ca05e105e49b357df141820926f1cd18)
* update go version [4a99abc](https://github.com/dssutg/bump-version/commit/4a99abc0f9c68eed659aab9fbdd0fc44aec7b762)

## [1.0.0](///compare/v0.3.0...v1.0.0) (2025-11-07)


### Features

* **config:** [BREAKING CHANGE] change default version filename list to empty 5fab50a

## [0.3.0](///compare/v0.2.0...v0.3.0) (2025-09-23)


### Features

* add a better error message if no tags exist when bumping that guides the user to run `git tag` 0beac0d
* **command:** add the cancel command to cancel recent version bump if done too early c6641e8
* **config:** add shouldPushToOrigin config property 2225645

## [0.2.0](///compare/v0.1.4...v0.2.0) (2025-09-18)


### Features

* add revert commits support and add more commit message checks 6f9b374
* **config:** add "bumpVersionCommit" config property 22aeae8
* replace regexp conventional commit parser with a full-featured parser (better errors, extensible) 0d6fb6d


### Bug Fixes

* embed utility's VERSION file to not require it alongside the program 263b5b1
* ensure the utility cd's back to Git repo root path even if started in deeper levels dcc6009

## [0.1.4](///compare/v0.1.3...v0.1.4) (2025-09-16)


### Bug Fixes

* ensure the version cannot be bumped if there's no commits that can increment it cc80e11

## [0.1.3](///compare/v0.1.2...v0.1.3) (2025-09-16)


### Bug Fixes

* ensure version numbers in version, my-version, next-version commands are printed as X.Y.Z 4b65de8

## [0.1.2](///compare/v0.1.1...v0.1.2) (2025-09-16)


### Bug Fixes

* **usage:** ensure option descriptions have proper alignment f1bbf77

## [0.1.1](///compare/v0.1.0...v0.1.1) (2025-09-16)


### Bug Fixes

* ensure the lint and lint-all commands use the regexp that considers Git hash 8000879

## [0.1.0](///compare/v0.0.1...v0.1.0) (2025-09-16)


### Features


* add the initial implementation of the utility ac773e5










