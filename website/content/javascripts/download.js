// Download buttons: find the newest release in the visitor's browser and point each button at the
// file whose name ends with the button's data-asset text (for example "-windows-x64.msi").
// Without JavaScript, offline, or when GitHub's rate limit is reached, every button keeps its
// plain link to the releases page. Nothing is sent to GitHub except this one ordinary request.
(function () {
  var API = "https://api.github.com/repos/chromascope-ms/chromascope/releases/latest";

  function setStatus(text) {
    var nodes = document.querySelectorAll("[data-download-status]");
    for (var i = 0; i < nodes.length; i++) {
      nodes[i].textContent = text;
    }
  }

  function megabytes(bytes) {
    return (bytes / (1024 * 1024)).toFixed(0) + " MB";
  }

  function run() {
    var buttons = document.querySelectorAll("a[data-asset]");
    if (!buttons.length || !window.fetch) {
      return;
    }
    fetch(API, { headers: { Accept: "application/vnd.github+json" } })
      .then(function (response) {
        if (!response.ok) {
          throw new Error("HTTP " + response.status);
        }
        return response.json();
      })
      .then(function (release) {
        var assets = release.assets || [];
        var matched = 0;
        for (var i = 0; i < buttons.length; i++) {
          var wanted = buttons[i].getAttribute("data-asset");
          for (var j = 0; j < assets.length; j++) {
            if (assets[j].name.slice(-wanted.length) === wanted) {
              buttons[i].href = assets[j].browser_download_url;
              buttons[i].title = assets[j].name + " (" + megabytes(assets[j].size) + ")";
              matched++;
              break;
            }
          }
          if (!buttons[i].title) {
            buttons[i].href = release.html_url || buttons[i].href;
          }
        }
        if (matched > 0) {
          setStatus("Newest release: " + release.tag_name + ". The buttons download its files directly.");
        } else {
          setStatus(
            "The newest release is " + release.tag_name +
            ". Its files are named differently from the ones described here, so the buttons open its release page."
          );
        }
      })
      .catch(function () {
        setStatus("The newest release could not be looked up from here, so the buttons open the releases page.");
      });
  }

  if (window.document$ && window.document$.subscribe) {
    window.document$.subscribe(run);
  } else if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", run);
  } else {
    run();
  }
})();
