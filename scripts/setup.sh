#!/bin/bash

# HFT RTB Setup Script
# This script helps set up the development environment and dependencies

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Logging functions
log() {
    echo -e "${BLUE}[$(date +'%Y-%m-%d %H:%M:%S')]${NC} $1"
}

error() {
    echo -e "${RED}[ERROR]${NC} $1" >&2
}

success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

# Configuration
PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# Help function
show_help() {
    cat << EOF
HFT RTB Setup Script

Usage: $0 [COMMAND] [OPTIONS]

Commands:
    deps                Install system dependencies
    rust                Install/update Rust toolchain
    docker              Setup Docker environment
    dev                 Setup development environment
    all                 Run all setup steps
    check               Check system requirements
    help                Show this help message

Options:
    --verbose           Enable verbose output
    --skip-docker       Skip Docker setup
    --skip-rust         Skip Rust installation

Examples:
    $0 all                      # Complete setup
    $0 deps                     # Install system dependencies only
    $0 dev --skip-docker        # Setup dev environment without Docker

EOF
}

# Check if command exists
command_exists() {
    command -v "$1" >/dev/null 2>&1
}

# Check system requirements
check_requirements() {
    log "Checking system requirements..."
    
    local missing_deps=()
    
    # Check OS
    case "$(uname -s)" in
        Linux*)     OS=Linux;;
        Darwin*)    OS=Mac;;
        CYGWIN*)    OS=Cygwin;;
        MINGW*)     OS=MinGw;;
        *)          OS="UNKNOWN:$(uname -s)"
    esac
    
    log "Detected OS: $OS"
    
    # Check required commands
    local required_commands=("git" "curl" "pkg-config")
    
    for cmd in "${required_commands[@]}"; do
        if ! command_exists "$cmd"; then
            missing_deps+=("$cmd")
        fi
    done
    
    # Check optional but recommended commands
    local optional_commands=("docker" "docker-compose" "grpcurl")
    local missing_optional=()
    
    for cmd in "${optional_commands[@]}"; do
        if ! command_exists "$cmd"; then
            missing_optional+=("$cmd")
        fi
    done
    
    if [ ${#missing_deps[@]} -eq 0 ]; then
        success "All required dependencies are installed"
    else
        error "Missing required dependencies: ${missing_deps[*]}"
        return 1
    fi
    
    if [ ${#missing_optional[@]} -gt 0 ]; then
        warning "Missing optional dependencies: ${missing_optional[*]}"
    fi
    
    # Check Rust
    if command_exists "rustc"; then
        local rust_version=$(rustc --version | cut -d' ' -f2)
        log "Rust version: $rust_version"
        
        # Check if version is >= 1.75
        local required_version="1.75.0"
        if [ "$(printf '%s\n' "$required_version" "$rust_version" | sort -V | head -n1)" = "$required_version" ]; then
            success "Rust version is compatible"
        else
            warning "Rust version $rust_version is older than required $required_version"
        fi
    else
        warning "Rust is not installed"
    fi
    
    # Check Docker
    if command_exists "docker"; then
        if docker info >/dev/null 2>&1; then
            success "Docker is running"
        else
            warning "Docker is installed but not running"
        fi
    else
        warning "Docker is not installed"
    fi
}

# Install system dependencies
install_deps() {
    log "Installing system dependencies..."
    
    case "$OS" in
        Linux)
            if command_exists "apt-get"; then
                log "Using apt-get package manager..."
                sudo apt-get update
                sudo apt-get install -y \
                    build-essential \
                    pkg-config \
                    libssl-dev \
                    ca-certificates \
                    curl \
                    git \
                    netcat-openbsd
            elif command_exists "yum"; then
                log "Using yum package manager..."
                sudo yum groupinstall -y "Development Tools"
                sudo yum install -y \
                    pkgconfig \
                    openssl-devel \
                    ca-certificates \
                    curl \
                    git \
                    nc
            elif command_exists "pacman"; then
                log "Using pacman package manager..."
                sudo pacman -S --noconfirm \
                    base-devel \
                    pkg-config \
                    openssl \
                    ca-certificates \
                    curl \
                    git \
                    gnu-netcat
            else
                error "Unsupported Linux distribution"
                return 1
            fi
            ;;
        Mac)
            if command_exists "brew"; then
                log "Using Homebrew package manager..."
                brew install pkg-config openssl curl git netcat
            else
                warning "Homebrew not found. Please install Homebrew first: https://brew.sh/"
                return 1
            fi
            ;;
        *)
            error "Unsupported operating system: $OS"
            return 1
            ;;
    esac
    
    success "System dependencies installed"
}

# Install/update Rust
install_rust() {
    log "Setting up Rust toolchain..."
    
    if ! command_exists "rustup"; then
        log "Installing Rust via rustup..."
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
        source "$HOME/.cargo/env"
    else
        log "Updating Rust toolchain..."
        rustup update
    fi
    
    # Install required components
    log "Installing Rust components..."
    rustup component add clippy rustfmt
    
    # Install useful cargo tools
    log "Installing cargo tools..."
    cargo install --locked cargo-watch cargo-audit cargo-outdated 2>/dev/null || warning "Some cargo tools failed to install"
    
    success "Rust toolchain setup complete"
}

# Setup Docker environment
setup_docker() {
    log "Setting up Docker environment..."
    
    if ! command_exists "docker"; then
        case "$OS" in
            Linux)
                log "Installing Docker on Linux..."
                curl -fsSL https://get.docker.com -o get-docker.sh
                sudo sh get-docker.sh
                sudo usermod -aG docker "$USER"
                rm get-docker.sh
                warning "Please log out and back in for Docker group membership to take effect"
                ;;
            Mac)
                warning "Please install Docker Desktop for Mac from: https://docs.docker.com/desktop/mac/install/"
                return 1
                ;;
            *)
                error "Docker installation not supported for $OS"
                return 1
                ;;
        esac
    fi
    
    if ! command_exists "docker-compose"; then
        log "Installing Docker Compose..."
        case "$OS" in
            Linux)
                sudo curl -L "https://github.com/docker/compose/releases/latest/download/docker-compose-$(uname -s)-$(uname -m)" -o /usr/local/bin/docker-compose
                sudo chmod +x /usr/local/bin/docker-compose
                ;;
            Mac)
                if command_exists "brew"; then
                    brew install docker-compose
                else
                    warning "Please install Docker Compose manually"
                    return 1
                fi
                ;;
        esac
    fi
    
    # Test Docker
    if docker info >/dev/null 2>&1; then
        success "Docker is working"
    else
        warning "Docker is installed but not running. Please start Docker daemon."
    fi
}

# Setup development environment
setup_dev() {
    log "Setting up development environment..."
    
    cd "$PROJECT_ROOT"
    
    # Create necessary directories
    log "Creating project directories..."
    mkdir -p logs data tmp
    
    # Build the project to check everything works
    log "Building project..."
    if cargo build; then
        success "Project builds successfully"
    else
        error "Project build failed"
        return 1
    fi
    
    # Run tests
    log "Running tests..."
    if cargo test --lib; then
        success "Tests pass"
    else
        warning "Some tests failed"
    fi
    
    # Setup git hooks (if in git repo)
    if [ -d ".git" ]; then
        log "Setting up git hooks..."
        cat > .git/hooks/pre-commit << 'EOF'
#!/bin/bash
# Pre-commit hook for HFT RTB project

set -e

echo "Running pre-commit checks..."

# Format check
if ! cargo fmt -- --check; then
    echo "Code formatting issues found. Run 'cargo fmt' to fix."
    exit 1
fi

# Clippy check
if ! cargo clippy -- -D warnings; then
    echo "Clippy warnings found. Please fix them."
    exit 1
fi

# Test check
if ! cargo test --lib; then
    echo "Tests failed. Please fix them."
    exit 1
fi

echo "Pre-commit checks passed!"
EOF
        chmod +x .git/hooks/pre-commit
        success "Git hooks installed"
    fi
    
    success "Development environment setup complete"
}

# Parse command line arguments
COMMAND=""
VERBOSE=false
SKIP_DOCKER=false
SKIP_RUST=false

while [[ $# -gt 0 ]]; do
    case $1 in
        deps|rust|docker|dev|all|check|help)
            COMMAND="$1"
            shift
            ;;
        --verbose)
            VERBOSE=true
            shift
            ;;
        --skip-docker)
            SKIP_DOCKER=true
            shift
            ;;
        --skip-rust)
            SKIP_RUST=true
            shift
            ;;
        *)
            error "Unknown option: $1"
            show_help
            exit 1
            ;;
    esac
done

# Set verbose mode
if [ "$VERBOSE" = true ]; then
    set -x
fi

# Main execution
main() {
    case "$COMMAND" in
        check)
            check_requirements
            ;;
        deps)
            install_deps
            ;;
        rust)
            if [ "$SKIP_RUST" = false ]; then
                install_rust
            else
                log "Skipping Rust installation"
            fi
            ;;
        docker)
            if [ "$SKIP_DOCKER" = false ]; then
                setup_docker
            else
                log "Skipping Docker setup"
            fi
            ;;
        dev)
            setup_dev
            ;;
        all)
            check_requirements
            install_deps
            if [ "$SKIP_RUST" = false ]; then
                install_rust
            fi
            if [ "$SKIP_DOCKER" = false ]; then
                setup_docker
            fi
            setup_dev
            success "Complete setup finished!"
            ;;
        help|"")
            show_help
            ;;
        *)
            error "Unknown command: $COMMAND"
            show_help
            exit 1
            ;;
    esac
}

# Run main function
main