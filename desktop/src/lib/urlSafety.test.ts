import { describe, expect, it } from "vitest";
import { checkUrlSafety } from "./urlSafety";

describe("checkUrlSafety", () => {
  it("accepts a real https model page", () => {
    const result = checkUrlSafety("https://huggingface.co/Qwen/Qwen2.5-7B-Instruct-GGUF");
    expect(result.safe).toBe(true);
    expect(result.hostname).toBe("huggingface.co");
  });

  it("rejects localhost", () => {
    expect(checkUrlSafety("http://localhost:1420/").safe).toBe(false);
    expect(checkUrlSafety("http://localhost/").reason).toBe("url_loopback_or_private");
  });

  it("rejects loopback IPs and private ranges", () => {
    expect(checkUrlSafety("http://127.0.0.1:8080/").safe).toBe(false);
    expect(checkUrlSafety("http://192.168.1.5/").safe).toBe(false);
    expect(checkUrlSafety("http://10.0.0.1/").safe).toBe(false);
    expect(checkUrlSafety("http://[::1]/").safe).toBe(false);
  });

  it("rejects file:// and javascript: schemes", () => {
    expect(checkUrlSafety("file:///etc/passwd").reason).toBe("url_scheme_not_allowed");
    expect(checkUrlSafety("javascript:alert(1)").safe).toBe(false);
  });

  it("rejects malformed URLs", () => {
    expect(checkUrlSafety("not a url").reason).toBe("url_malformed");
    expect(checkUrlSafety("").reason).toBe("url_empty");
  });
});
