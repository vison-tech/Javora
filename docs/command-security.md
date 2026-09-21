# Command security

Javora executes approved build, test, and inspection commands without invoking a
shell. Commands are parsed into a fixed allowlist and launched with
`Command::new(...).args(...)`, so shell expansion, pipelines, redirects, and
command substitution are not available.

Command arguments that use absolute paths, parent-directory traversal, or
Windows drive-root paths are rejected so inspection remains project-scoped.

## Allowed commands

- Maven: `mvn` or `./mvnw` with `clean`, `compile`, `test`, `package`, `install`,
  or `verify`.
- Gradle: `gradle` or `./gradlew` with `build`, `test`, `assemble`, `check`, or
  `clean`.
- Git inspection: `status`, `log`, `diff`, `branch`, and `show`.
- File inspection: `ls`, `cat`, `find`, `grep`, `tree`, `head`, and `tail`.
- Java inspection: version queries for `java`, `javac`, and `jshell`, plus
  `javap` without JVM forwarding options.

Git arguments that can write output files, invoke external helpers, or modify
branches are rejected. Mutating or command-executing `find` actions such as
`-delete`, `-exec`, and `-fprint` are also rejected.

Every accepted command is shown to the user for confirmation before it runs.
Commands are terminated after 120 seconds, including their process group on
Unix or process tree on Windows.

## Trust boundary

The allowlist prevents shell injection and blocks known write paths in commands
described as read-only. It is not an operating-system sandbox. Maven and Gradle
execute build logic from the project, and that logic can run arbitrary code with
the current user's permissions.

Use a container or another OS-level sandbox when opening an untrusted project.
For example, mount source read-only where practical, expose only required build
caches, and disable network access for offline verification.

## Extending the policy

When adding a command or argument:

1. Classify whether it reads data, writes data, or executes project-controlled
   code.
2. Add the narrowest parser rule that supports the intended workflow.
3. Keep execution parameterized and do not route the command through a shell.
4. Verify mutation flags, output-file options, helper hooks, timeout behavior,
   and wrapper handling.
5. Update this document with the resulting trust boundary.
