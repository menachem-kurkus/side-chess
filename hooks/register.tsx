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
      const opened = await $.process.run(['open', '-a', 'Side Chess'])
      if (opened.exitCode === 0) {
        void $.prompt.submit({
          text: 'I have begun a Side Chess game. Use the side-chess skill and play black.',
        })
        return
      }
      $.ui.toast('Side Chess is not installed yet: asking Claude to build it')
      void $.prompt.submit({
        text:
          'Side Chess is not installed yet. Use the side-chess skill: install it with ' +
          `bash "${root}/app/install.sh", open it, then play black.`,
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
