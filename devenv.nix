{ ... }:

{
  languages.rust.enable = true;

  profiles = {
    stable.module = {
      languages.rust.channel = "stable";
    };

    nightly.module = {
      languages.rust.channel = "nightly";
    };
  };
}
