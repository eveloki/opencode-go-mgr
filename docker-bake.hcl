variable "SMOKE_IMAGE" {
  default = "ocg-manager:smoke"
}

variable "SMOKE_BROWSER_IMAGE" {
  default = "ocg-browser:smoke"
}

group "smoke" {
  targets = ["manager-smoke", "browser-smoke"]
}

target "manager-smoke" {
  context = "."
  dockerfile = "Dockerfile"
  platforms = ["linux/amd64"]
  tags = ["${SMOKE_IMAGE}"]
  cache-from = ["type=gha,scope=ocg-manager-linux-amd64"]
}

target "browser-smoke" {
  context = "."
  dockerfile = "Dockerfile.browser"
  platforms = ["linux/amd64"]
  tags = ["${SMOKE_BROWSER_IMAGE}"]
  cache-from = ["type=gha,scope=ocg-browser-linux-amd64"]
}
