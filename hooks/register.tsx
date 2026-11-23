import type { Register } from 'claude-code'

// A "Launch Side Chess" button above the prompt. It opens the app and tells
// this session the game has begun, so Claude plays black (see the side-chess skill).
export const register: Register = on => {
  on('ui.render', { component: 'AbovePrompt' }, ($, e, next) => {
    if (e.props.hasSurvey) {
      return next(e)
    }

    const { Box, Button, Text } = $.ui.resolve(e)

    const launch = async () => {
      const env = await $.process.hookEnv()
      const root = env.CLAUDE_PLUGIN_ROOT ?? ''
      // Linux: start ~/.local/bin/side-chess. macOS: open the app. Windows: start the installed program.
      const attempt = async (cmd: string[]) => {
        try {
          return (await $.process.run(cmd)).exitCode === 0
        } catch {
          return false
        }
      }
      const opened =
        (await attempt([
          'sh',
          '-c',
          '[ "$(uname)" = Linux ] && [ -x "$HOME/.local/bin/side-chess" ] && { nohup "$HOME/.local/bin/side-chess" >/dev/null 2>&1 & }',
        ])) ||
        (await attempt(['open', '-a', 'Side Chess'])) ||
        (await attempt([
          'powershell',
          '-NoProfile',
          '-Command',
          'Start-Process (Join-Path $env:LOCALAPPDATA "Programs/Side Chess/side-chess.exe")',
        ]))
      if (opened) {
        void $.prompt.submit({
          text: 'I have begun a Side Chess game. Use the side-chess skill and play black.',
        })
        return
      }
      $.ui.toast('Side Chess is not installed yet: asking Claude to build it')
      void $.prompt.submit({
        text:
          'Side Chess is not installed yet. Use the side-chess skill: install it with ' +
          `bash "${root}/app/install.sh" on a Mac or Linux, or ` +
          `powershell -ExecutionPolicy Bypass -File "${root}/app/install.ps1" on Windows, open it, then play black.`,
      })
    }

    return (
      <Box>
        <Text dimColor>Side Chess </Text>
        <Button key="launch" label="Launch Side Chess" onPress={launch} />
      </Box>
    )
  })
}
