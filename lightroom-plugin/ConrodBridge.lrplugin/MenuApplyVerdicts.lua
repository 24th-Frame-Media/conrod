-- Library > Plug-in Extras > Apply verdicts from Conrod
local LrPathUtils = import 'LrPathUtils'
local Bridge = dofile(LrPathUtils.child(_PLUGIN.path, 'ConrodBridge.lua'))
Bridge.applyVerdicts()
