/* PDF-only: fill the title page date at render time.
   Loaded only by the mkdocs.print.yml build (headless Chrome executes it). */
document.addEventListener("DOMContentLoaded", function () {
  document.querySelectorAll(".print-date").forEach(function (el) {
    el.textContent = new Date().toLocaleDateString("de-DE", {
      day: "2-digit",
      month: "long",
      year: "numeric",
    });
  });
});