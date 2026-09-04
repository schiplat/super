# ==========================================
# SuperProcess OSS Build System
# ==========================================

TARGET_DIR   := $(if $(CARGO_TARGET_DIR),$(CARGO_TARGET_DIR)/release,target/release)
# Define all OSS binaries (Server 'superd' and CLI 'super')
BINARIES     := --bin superd --bin super

# Colors for output
GREEN  := \033[0;32m
YELLOW := \033[0;33m
BLUE   := \033[0;34m
NC     := \033[0m # No Color

# Default target
.PHONY: all
all: build

# Help message
.PHONY: help
help:
	@echo "Usage: make [target]"
	@echo ""
	@echo "Targets:"
	@echo "  build       Build OSS binaries (superd + super CLI)"
	@echo "  fetch-keys  Local/debug: fetch Manager keyring into common/keys/ (do not commit by default)"
	@echo "  clean       Clean up build artifacts (target/)"
	@echo "  check       Run cargo check"
	@echo "  docker      Build schiplat/super image (native arch, local load)"
	@echo "  docker-multi  Build and push linux/amd64 image"
	@echo ""
	@echo "Daily: make build uses hand-curated common/keys/*.public.key (no Manager)."
	@echo "Release CI fetches Manager keyring before packaging (build tree only)."
	@echo ""

# ==========================================
# Core Build Tasks
# ==========================================

.PHONY: fetch-keys
fetch-keys:
	@echo "$(BLUE)🔑 Fetching verifying keyring from Manager...$(NC)"
	@REQUIRE_MANAGER_KEYRING=1 bash .github/scripts/fetch-verifying-keys.sh

.PHONY: build
build:
	@echo "$(BLUE)🦀 Building Rust Binaries (OSS)...$(NC)"
	@cargo build --release $(BINARIES)
	@echo "$(GREEN)🎉 All OSS binaries built successfully!$(NC)"
	@echo "📂 Locations:"
	@echo "   - Server: $(TARGET_DIR)/superd"
	@echo "   - CLI:    $(TARGET_DIR)/super"

# ==========================================
# Helper Tasks
# ==========================================

.PHONY: clean
clean:
	@echo "$(YELLOW)🧹 Cleaning up...$(NC)"
	@cargo clean
	@echo "$(GREEN)✅ Clean complete.$(NC)"

.PHONY: check
check:
	@cargo check

# Local docs preview (Hugo adjusts paths for localhost — do not open public/ as files)
.PHONY: docs-serve
docs-serve:
	@echo "$(BLUE)📖 Docs preview → http://localhost:1313/$(NC)"
	cd docs && hugo server -D --disableFastRender

# Docker image (build context = repo root)
DOCKER_IMAGE := schiplat/super:latest
DOCKERFILE   := packaging/docker/Dockerfile
DOCKER_PLATFORMS := linux/amd64

.PHONY: docker
docker:
	@echo "$(BLUE)🐳 Building $(DOCKER_IMAGE) (native arch)...$(NC)"
	docker buildx build --load -f $(DOCKERFILE) -t $(DOCKER_IMAGE) .
	@echo "$(GREEN)✅ Docker image ready: $(DOCKER_IMAGE)$(NC)"

.PHONY: docker-multi
docker-multi:
	@echo "$(BLUE)🐳 Building $(DOCKER_IMAGE) for $(DOCKER_PLATFORMS)...$(NC)"
	docker buildx build --platform $(DOCKER_PLATFORMS) -f $(DOCKERFILE) -t $(DOCKER_IMAGE) --push .
	@echo "$(GREEN)✅ Image pushed: $(DOCKER_IMAGE) ($(DOCKER_PLATFORMS))$(NC)"
