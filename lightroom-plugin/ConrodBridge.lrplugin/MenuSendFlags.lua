-- Library > Plug-in Extras > Send flags to Conrod
local LrPathUtils = import 'LrPathUtils'
local Bridge = dofile(LrPathUtils.child(_PLUGIN.path, 'ConrodBridge.lua'))
Bridge.sendFlags()
