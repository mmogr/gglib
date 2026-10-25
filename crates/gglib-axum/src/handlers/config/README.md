# config

<!-- module-docs:start -->

Configuration and system management handlers.

Settings, system setup, and the two engines, llama.cpp and the image runtime
(stable-diffusion.cpp's `sd-server`): each installs its pinned release over
SSE with the same event names, reports its status, and is removed, and an
install or a removal is refused while a model runs on that engine. Each
engine's install and removal take one slot of its own (`install_slot`), so
neither starts while the other runs. The daemon installs downloads only: an
install the command line would build is refused, naming the command.

<!-- module-docs:end -->
