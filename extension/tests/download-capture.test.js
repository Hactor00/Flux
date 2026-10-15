import test from "node:test";
import assert from "node:assert/strict";
import { attachmentFilename, isAttachment } from "../download-capture.js";

test("attachment capture accepts only HTTP GET attachments", () => {
  const responseHeaders = [
    {
      name: "Content-Disposition",
      value: 'attachment; filename="archive.zip"',
    },
  ];
  assert.equal(
    isAttachment({
      method: "GET",
      url: "https://site.test/file",
      responseHeaders,
    }),
    true,
  );
  assert.equal(
    isAttachment({
      method: "POST",
      url: "https://site.test/file",
      responseHeaders,
    }),
    false,
  );
  assert.equal(
    isAttachment({
      method: "GET",
      url: "https://site.test/file",
      responseHeaders: [{ name: "Content-Disposition", value: "inline" }],
    }),
    false,
  );
  assert.equal(
    isAttachment({
      method: "GET",
      url: "blob:https://site.test/file",
      responseHeaders,
    }),
    false,
  );
});
test("attachment filenames decode UTF-8 and tolerate malformed encoding", () => {
  assert.equal(
    attachmentFilename([
      {
        name: "Content-Disposition",
        value:
          "attachment; filename=fallback.zip; filename*=UTF-8''T%C3%BCrk%C3%A7e.zip",
      },
    ]),
    "Türkçe.zip",
  );
  assert.equal(
    attachmentFilename([
      { name: "content-disposition", value: 'attachment; filename="film.mp4"' },
    ]),
    "film.mp4",
  );
  assert.equal(
    attachmentFilename([
      {
        name: "content-disposition",
        value: "attachment; filename*=UTF-8''%GG; filename=file.zip",
      },
    ]),
    "file.zip",
  );
});
