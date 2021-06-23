let
  src = (builtins.fromJSON (builtins.readFile ./flake.lock)).nodes.flakeCompat.locked;
  compat = import (fetchTarball { url = "https://github.com/edolstra/flake-compat/archive/${src.rev}.tar.gz"; sha256 = src.narHash; });
in
(compat { src = ./.; }).shellNix.default
