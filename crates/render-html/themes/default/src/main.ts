// The default theme's scripts: defines its elements. Loaded as a module
// by every page (base.html), after the page's import map.

import { define as copy } from "./copy.js";
import { define as palette } from "./palette.js";
import { define as tabs } from "./tabs.js";
import { define as toc } from "./toc.js";

palette();
copy();
tabs();
toc();
