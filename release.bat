@echo off
rem A whole Windows release in one command: release.bat --release 0.8.0
rem Pulls, sets the version if it's new, runs the checks, builds the MSI and setup .exe, builds the Linux packages in
rem Docker, and puts them all in the GitHub release (a draft). Everything is done by scripts\release-all.mjs, which
rem lists the steps and the other options (--skip-checks, --no-linux, --final); this only runs it. The Mac's is
rem release.sh. docs\RELEASE.md has how the two fit together.
cd /d "%~dp0"
node scripts\release-all.mjs %*
exit /b %ERRORLEVEL%
