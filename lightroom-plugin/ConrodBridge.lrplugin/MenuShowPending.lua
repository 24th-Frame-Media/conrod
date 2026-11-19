-- Library > Plug-in Extras > Show pending packages
local LrPathUtils = import 'LrPathUtils'
local Bridge = dofile(LrPathUtils.child(_PLUGIN.path, 'ConrodBridge.lua'))
Bridge.showPending()
