# Serial protocol for communicating with the signal controller

The serial protocol follows a simple format, where each command is separated by arbitrary newline characters (blank lines are allowed, both Windows and Linux line endings allowed). Within a command line, the following format is used:

```
[Signal ID]:[Signal state]#[Comments]
```

The signal ID serves to differentiate different signal controllers, which may all be listening on the same serial connection. The signal ID corresponds to the control box’s identifier for the signal, such as `A` for the first station entry signal or `P2` for an intermediate signal on track 2.

The signal ID is separated by the signal state command with a colon. The following commands are currently supported for H/V signals:

- `0`: Switch to Hp0, i.e. Stop.
- `1`: Switch to Hp1, i.e. Proceed.
- `2`: Switch to Hp2, i.e. Proceed Slowly. Separate speed signaling control is currently not supported and may be added in the future; though the signal might of course have a fixed Zs3&Zs3v speed sign.
- `A`: Disable the signal, since it is not currently needed. A notification light will be illuminated (and one must exist for this command to succeed).
- `D`: Switch the signal completely dark, no lamps illuminated.

For Ks signals, the main (numbered) aspects have a different meaning:

- `0`: Switch to Hp0, Stop.
- `1`: Switch to Ks1, Proceed.
- `2`: Switch to Ks2, Expect Stop.

For compatibility, all characters beyond the first should be disregarded.

All characters including and beyond a hash are always disregarded, and may specify comments, especially when playing back pre-recorded signal commands that have been annotated with human-readable information or information for other systems. It is allowed for a line to include only a hash followed by comments.

If a signal controller is alone on the serial bus, it may send responses. The response format consists of the following single line sent back:

```
[Signal ID]:[Response state]:[Extra response info]#[Comments]
```

The signal ID is the same as the one that the signal was sent to, and mainly has the purpose to cross-check that the correct signal is responding.

The response state is either `A` for acknowledgement, if the command was executed successfully, or `E`, if the command was not executed successfully.

The extra response info for the acknowledgement contains the signal that was switched to, as a safeguard against corrupted information.

Extra response info may be included for `E` responses. They consist of a single digit identifying the type of error. If the error type is generic or unknown, no extra info should be sent back.

- `0`: Command format invalid. Signal state unchanged.
- `1`: Unsupported aspect: This signal cannot display the specified aspect. For instance, some main signals do not have a yellow lamp and therefore cannot display the Hp2 aspect. Signal state unchanged.
- `2`: Electrical failure with successful fallback. The signal was not able to enter the aspect due to electrical issues. It fell back to Stop aspect (Hp0) successfully (meaning that effectively, the command `[Signal ID]:0` was executed with response `A`).
- `3`: Electrical failure without fallback to Hp0. The signal was not able to enter the aspect due to electrical issues. It additionally was not able to fall back to the safe Stop aspect (Hp0) even though this was attempted. The signal instead fell back to completely dark (which is always possible e.g. by cutting power to all components), which under these circumstances counts as an invalid aspect. This error state is intended to allow the activation of further assistance signals like Zs1 or Zs7, or to reattempt a signal change at a later point.

Comments may be added by the controller, which must contain human-readable text. These might include further explanations for the error and are useful for manual troubleshooting.
