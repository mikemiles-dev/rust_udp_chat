# 0.1.11
 * Fixed terminal cursor disappearing after `/quit` command.

# 0.1.10
 * Increased file transfer size limit from 10MB to 100MB.
 * File transfers now require recipient acceptance. Sender uses `/send <user> <file>`, recipient must `/accept <sender>` or `/reject <sender>`.

# 0.1.9
 * Added client/server version checking. Clients with mismatched versions are disconnected with a link to upgrade instructions.

# 0.1.8
 * Status now persists across reconnections but is cleared on explicit `/quit`, kick, or ban.

# 0.1.7
 * Refactored command parsing to use shared command constants, eliminating duplication between command definitions and input parsing.

# 0.1.6
 * Better client reconnection.

# 0.1.5
 * More robust dead connection logic.
 * Fixed issues with input cursor and ctrl + c

# 0.1.4
 * Centralize completer logic.

# 0.1.3
 * Added User Status Command
 * Server checks for disconnect cleanup

# 0.1.2
 * Added file transfer 

