-- Development helper: runs the JSON self-test inside Lightroom.
--
-- This file is deliberately NOT listed in Info.lua, so it adds no menu item.
-- To run it, add this entry to LrLibraryMenuItems in Info.lua, reload the
-- plug-in in File > Plug-in Manager, then use the menu item:
--
--   { title = 'Run self-test', file = 'MenuSelftest.lua' },
--
-- The same tests run outside Lightroom with: lua Selftest.lua
local LrDialogs = import 'LrDialogs'
local LrPathUtils = import 'LrPathUtils'

local Selftest = dofile(LrPathUtils.child(_PLUGIN.path, 'Selftest.lua'))
local lines, failures = Selftest.run()

LrDialogs.message(
  'Conrod Bridge self-test',
  table.concat(lines, '\n'),
  failures == 0 and 'info' or 'critical'
)
