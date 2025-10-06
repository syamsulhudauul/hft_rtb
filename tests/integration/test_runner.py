#!/usr/bin/env python3
"""
HFT RTB Integration Test Runner with Advanced Reporting

This script orchestrates the execution of all integration tests and generates
comprehensive reports with metrics, performance analysis, and visualizations.
"""

import argparse
import asyncio
import json
import logging
import os
import subprocess
import sys
import time
import yaml
from datetime import datetime, timedelta
from pathlib import Path
from typing import Dict, List, Optional, Any, Tuple
import tempfile
import shutil

# Third-party imports (install with pip if needed)
try:
    import aiohttp
    import matplotlib.pyplot as plt
    import pandas as pd
    import seaborn as sns
    from jinja2 import Template
except ImportError as e:
    print(f"Missing required dependency: {e}")
    print("Install with: pip install aiohttp matplotlib pandas seaborn jinja2")
    sys.exit(1)

# Configure logging
logging.basicConfig(
    level=logging.INFO,
    format='%(asctime)s - %(name)s - %(levelname)s - %(message)s'
)
logger = logging.getLogger(__name__)

class TestResult:
    """Represents the result of a test execution."""
    
    def __init__(self, name: str, status: str, duration: float, 
                 details: Optional[Dict] = None, error: Optional[str] = None):
        self.name = name
        self.status = status  # 'passed', 'failed', 'skipped', 'error'
        self.duration = duration
        self.details = details or {}
        self.error = error
        self.timestamp = datetime.now()

class TestSuite:
    """Represents a collection of tests."""
    
    def __init__(self, name: str, test_type: str):
        self.name = name
        self.test_type = test_type
        self.results: List[TestResult] = []
        self.start_time: Optional[datetime] = None
        self.end_time: Optional[datetime] = None
        self.metrics: Dict[str, Any] = {}
    
    @property
    def duration(self) -> float:
        if self.start_time and self.end_time:
            return (self.end_time - self.start_time).total_seconds()
        return 0.0
    
    @property
    def passed_count(self) -> int:
        return sum(1 for r in self.results if r.status == 'passed')
    
    @property
    def failed_count(self) -> int:
        return sum(1 for r in self.results if r.status == 'failed')
    
    @property
    def skipped_count(self) -> int:
        return sum(1 for r in self.results if r.status == 'skipped')
    
    @property
    def error_count(self) -> int:
        return sum(1 for r in self.results if r.status == 'error')
    
    @property
    def total_count(self) -> int:
        return len(self.results)
    
    @property
    def success_rate(self) -> float:
        if self.total_count == 0:
            return 0.0
        return self.passed_count / self.total_count

class IntegrationTestRunner:
    """Main test runner class."""
    
    def __init__(self, config_path: Optional[str] = None):
        self.script_dir = Path(__file__).parent
        self.project_root = self.script_dir.parent.parent
        self.config = self._load_config(config_path)
        self.test_suites: List[TestSuite] = []
        self.start_time: Optional[datetime] = None
        self.end_time: Optional[datetime] = None
        self.reports_dir = self.script_dir / "reports"
        self.reports_dir.mkdir(exist_ok=True)
        
        # Session identifier
        self.session_id = datetime.now().strftime("%Y%m%d_%H%M%S")
        
    def _load_config(self, config_path: Optional[str]) -> Dict:
        """Load test configuration."""
        if config_path:
            config_file = Path(config_path)
        else:
            config_file = self.script_dir / "config" / "test_config.yaml"
        
        if config_file.exists():
            with open(config_file, 'r') as f:
                config = yaml.safe_load(f)
            
            # Use environment-specific config if specified
            env = os.getenv('TEST_ENV', 'default')
            if env in config:
                base_config = config.get('default', {})
                env_config = config[env]
                # Merge configs (env overrides default)
                merged_config = self._deep_merge(base_config, env_config)
                return merged_config
            else:
                return config.get('default', {})
        else:
            logger.warning(f"Config file not found: {config_file}, using defaults")
            return self._get_default_config()
    
    def _deep_merge(self, base: Dict, override: Dict) -> Dict:
        """Deep merge two dictionaries."""
        result = base.copy()
        for key, value in override.items():
            if key in result and isinstance(result[key], dict) and isinstance(value, dict):
                result[key] = self._deep_merge(result[key], value)
            else:
                result[key] = value
        return result
    
    def _get_default_config(self) -> Dict:
        """Get default configuration."""
        return {
            'hft': {
                'host': 'localhost',
                'gateway_port': 8080,
                'metrics_port': 9090,
                'admin_port': 9091
            },
            'kafka': {
                'brokers': 'localhost:9092'
            },
            'redis': {
                'url': 'redis://localhost:6379'
            },
            'test_params': {
                'default_timeout': 30
            }
        }
    
    async def check_services(self) -> bool:
        """Check if all required services are running."""
        logger.info("Checking required services...")
        
        services = [
            ('HFT Gateway', self.config['hft']['host'], self.config['hft']['gateway_port']),
            ('HFT Metrics', self.config['hft']['host'], self.config['hft']['metrics_port']),
            ('HFT Admin', self.config['hft']['host'], self.config['hft']['admin_port']),
        ]
        
        all_ok = True
        async with aiohttp.ClientSession(timeout=aiohttp.ClientTimeout(total=10)) as session:
            for service_name, host, port in services:
                try:
                    url = f"http://{host}:{port}/health"
                    async with session.get(url) as response:
                        if response.status == 200:
                            logger.info(f"✓ {service_name} is running")
                        else:
                            logger.error(f"✗ {service_name} returned status {response.status}")
                            all_ok = False
                except Exception as e:
                    logger.error(f"✗ {service_name} is not accessible: {e}")
                    all_ok = False
        
        return all_ok
    
    async def run_rust_tests(self) -> TestSuite:
        """Run Rust integration tests."""
        suite = TestSuite("Rust Integration Tests", "rust")
        suite.start_time = datetime.now()
        
        logger.info("Running Rust integration tests...")
        
        rust_dir = self.script_dir
        cargo_toml = rust_dir / "Cargo.toml"
        
        if not cargo_toml.exists():
            logger.warning("Rust tests not found, skipping...")
            suite.results.append(TestResult("rust_tests", "skipped", 0.0, 
                                           details={"reason": "Cargo.toml not found"}))
            suite.end_time = datetime.now()
            return suite
        
        try:
            # Build first
            build_start = time.time()
            build_result = subprocess.run(
                ["cargo", "build", "--release"],
                cwd=rust_dir,
                capture_output=True,
                text=True,
                timeout=300
            )
            build_duration = time.time() - build_start
            
            if build_result.returncode != 0:
                suite.results.append(TestResult("rust_build", "failed", build_duration,
                                               error=build_result.stderr))
                suite.end_time = datetime.now()
                return suite
            
            suite.results.append(TestResult("rust_build", "passed", build_duration))
            
            # Run tests
            test_start = time.time()
            test_cmd = [
                "cargo", "run", "--release", "--",
                "--host", self.config['hft']['host'],
                "--gateway-port", str(self.config['hft']['gateway_port']),
                "--metrics-port", str(self.config['hft']['metrics_port']),
                "--admin-port", str(self.config['hft']['admin_port']),
                "--kafka-brokers", self.config['kafka']['brokers'],
                "--redis-url", self.config['redis']['url'],
                "--test", "all",
                "--duration", "30"
            ]
            
            test_result = subprocess.run(
                test_cmd,
                cwd=rust_dir,
                capture_output=True,
                text=True,
                timeout=600
            )
            test_duration = time.time() - test_start
            
            if test_result.returncode == 0:
                suite.results.append(TestResult("rust_integration", "passed", test_duration,
                                               details={"output": test_result.stdout}))
            else:
                suite.results.append(TestResult("rust_integration", "failed", test_duration,
                                               error=test_result.stderr))
                
        except subprocess.TimeoutExpired:
            suite.results.append(TestResult("rust_tests", "error", 600.0,
                                           error="Test execution timed out"))
        except Exception as e:
            suite.results.append(TestResult("rust_tests", "error", 0.0,
                                           error=str(e)))
        
        suite.end_time = datetime.now()
        return suite
    
    async def run_python_tests(self, test_type: str = "all") -> TestSuite:
        """Run Python integration tests."""
        suite = TestSuite(f"Python Tests ({test_type})", "python")
        suite.start_time = datetime.now()
        
        logger.info(f"Running Python integration tests ({test_type})...")
        
        python_dir = self.script_dir / "python"
        requirements_file = python_dir / "requirements.txt"
        
        if not requirements_file.exists():
            logger.warning("Python tests not found, skipping...")
            suite.results.append(TestResult("python_tests", "skipped", 0.0,
                                           details={"reason": "requirements.txt not found"}))
            suite.end_time = datetime.now()
            return suite
        
        try:
            # Setup virtual environment if needed
            venv_dir = python_dir / "venv"
            if not venv_dir.exists():
                logger.info("Creating Python virtual environment...")
                subprocess.run([sys.executable, "-m", "venv", str(venv_dir)], check=True)
            
            # Determine python executable in venv
            if os.name == 'nt':  # Windows
                python_exe = venv_dir / "Scripts" / "python.exe"
                pip_exe = venv_dir / "Scripts" / "pip.exe"
            else:  # Unix-like
                python_exe = venv_dir / "bin" / "python"
                pip_exe = venv_dir / "bin" / "pip"
            
            # Install dependencies
            install_start = time.time()
            install_result = subprocess.run(
                [str(pip_exe), "install", "-q", "-r", str(requirements_file)],
                capture_output=True,
                text=True,
                timeout=300
            )
            install_duration = time.time() - install_start
            
            if install_result.returncode != 0:
                suite.results.append(TestResult("python_deps", "failed", install_duration,
                                               error=install_result.stderr))
                suite.end_time = datetime.now()
                return suite
            
            suite.results.append(TestResult("python_deps", "passed", install_duration))
            
            # Set environment variables
            env = os.environ.copy()
            env.update({
                'HFT_HOST': self.config['hft']['host'],
                'HFT_GATEWAY_PORT': str(self.config['hft']['gateway_port']),
                'HFT_METRICS_PORT': str(self.config['hft']['metrics_port']),
                'HFT_ADMIN_PORT': str(self.config['hft']['admin_port']),
                'KAFKA_BROKERS': self.config['kafka']['brokers'],
                'REDIS_URL': self.config['redis']['url']
            })
            
            # Prepare pytest arguments
            pytest_args = [str(python_exe), "-m", "pytest", "-v"]
            
            # Add markers based on test type
            if test_type == "quick":
                pytest_args.extend(["-m", "not slow and not load"])
            elif test_type == "performance":
                pytest_args.extend(["-m", "performance"])
            elif test_type == "load":
                pytest_args.extend(["-m", "load"])
            elif test_type in ["health", "api", "kafka"]:
                pytest_args.extend(["-m", test_type])
            
            # Add reporting options
            report_xml = self.reports_dir / f"pytest_{self.session_id}_{test_type}.xml"
            report_html = self.reports_dir / f"pytest_{self.session_id}_{test_type}.html"
            
            pytest_args.extend([
                "--junit-xml", str(report_xml),
                "--html", str(report_html),
                "--self-contained-html",
                "--tb=short",
                "--durations=10"
            ])
            
            # Run tests
            test_start = time.time()
            test_result = subprocess.run(
                pytest_args,
                cwd=python_dir,
                capture_output=True,
                text=True,
                env=env,
                timeout=1800  # 30 minutes
            )
            test_duration = time.time() - test_start
            
            # Parse results from output
            output_lines = test_result.stdout.split('\n')
            for line in output_lines:
                if '::' in line and ('PASSED' in line or 'FAILED' in line or 'SKIPPED' in line):
                    parts = line.split()
                    test_name = parts[0].split('::')[-1]
                    status = parts[-1].lower()
                    
                    # Extract duration if available
                    duration = 0.0
                    for part in parts:
                        if 's' in part and part.replace('.', '').replace('s', '').isdigit():
                            try:
                                duration = float(part.replace('s', ''))
                                break
                            except ValueError:
                                pass
                    
                    suite.results.append(TestResult(test_name, status, duration))
            
            # If no individual results parsed, add summary result
            if not suite.results or len(suite.results) == 1:  # Only deps result
                if test_result.returncode == 0:
                    suite.results.append(TestResult(f"python_{test_type}", "passed", test_duration,
                                                   details={"output": test_result.stdout}))
                else:
                    suite.results.append(TestResult(f"python_{test_type}", "failed", test_duration,
                                                   error=test_result.stderr))
            
        except subprocess.TimeoutExpired:
            suite.results.append(TestResult("python_tests", "error", 1800.0,
                                           error="Test execution timed out"))
        except Exception as e:
            suite.results.append(TestResult("python_tests", "error", 0.0,
                                           error=str(e)))
        
        suite.end_time = datetime.now()
        return suite
    
    async def collect_system_metrics(self) -> Dict[str, Any]:
        """Collect system metrics during test execution."""
        metrics = {}
        
        try:
            # Get HFT metrics
            metrics_url = f"http://{self.config['hft']['host']}:{self.config['hft']['metrics_port']}/metrics"
            async with aiohttp.ClientSession() as session:
                async with session.get(metrics_url) as response:
                    if response.status == 200:
                        metrics_text = await response.text()
                        metrics['hft_metrics'] = self._parse_prometheus_metrics(metrics_text)
        except Exception as e:
            logger.warning(f"Failed to collect HFT metrics: {e}")
        
        return metrics
    
    def _parse_prometheus_metrics(self, metrics_text: str) -> Dict[str, float]:
        """Parse Prometheus metrics text."""
        metrics = {}
        for line in metrics_text.split('\n'):
            if line and not line.startswith('#'):
                parts = line.split()
                if len(parts) >= 2:
                    metric_name = parts[0]
                    try:
                        metric_value = float(parts[1])
                        metrics[metric_name] = metric_value
                    except ValueError:
                        pass
        return metrics
    
    def generate_html_report(self) -> str:
        """Generate comprehensive HTML report."""
        template_str = """
<!DOCTYPE html>
<html>
<head>
    <title>HFT RTB Integration Test Report</title>
    <style>
        body { font-family: Arial, sans-serif; margin: 20px; }
        .header { background: #f0f0f0; padding: 20px; border-radius: 5px; }
        .summary { display: flex; gap: 20px; margin: 20px 0; }
        .metric-card { background: #f9f9f9; padding: 15px; border-radius: 5px; flex: 1; }
        .passed { color: #28a745; }
        .failed { color: #dc3545; }
        .skipped { color: #ffc107; }
        .error { color: #6c757d; }
        table { width: 100%; border-collapse: collapse; margin: 20px 0; }
        th, td { border: 1px solid #ddd; padding: 8px; text-align: left; }
        th { background-color: #f2f2f2; }
        .suite-header { background: #e9ecef; font-weight: bold; }
        .chart-container { margin: 20px 0; text-align: center; }
    </style>
</head>
<body>
    <div class="header">
        <h1>HFT RTB Integration Test Report</h1>
        <p><strong>Session ID:</strong> {{ session_id }}</p>
        <p><strong>Generated:</strong> {{ timestamp }}</p>
        <p><strong>Duration:</strong> {{ total_duration }}s</p>
        <p><strong>Environment:</strong> {{ environment }}</p>
    </div>
    
    <div class="summary">
        <div class="metric-card">
            <h3>Total Tests</h3>
            <div style="font-size: 2em; font-weight: bold;">{{ total_tests }}</div>
        </div>
        <div class="metric-card">
            <h3>Passed</h3>
            <div style="font-size: 2em; font-weight: bold;" class="passed">{{ total_passed }}</div>
        </div>
        <div class="metric-card">
            <h3>Failed</h3>
            <div style="font-size: 2em; font-weight: bold;" class="failed">{{ total_failed }}</div>
        </div>
        <div class="metric-card">
            <h3>Success Rate</h3>
            <div style="font-size: 2em; font-weight: bold;">{{ success_rate }}%</div>
        </div>
    </div>
    
    <h2>Test Suites</h2>
    <table>
        <thead>
            <tr>
                <th>Suite</th>
                <th>Type</th>
                <th>Tests</th>
                <th>Passed</th>
                <th>Failed</th>
                <th>Skipped</th>
                <th>Duration</th>
                <th>Success Rate</th>
            </tr>
        </thead>
        <tbody>
            {% for suite in test_suites %}
            <tr>
                <td>{{ suite.name }}</td>
                <td>{{ suite.test_type }}</td>
                <td>{{ suite.total_count }}</td>
                <td class="passed">{{ suite.passed_count }}</td>
                <td class="failed">{{ suite.failed_count }}</td>
                <td class="skipped">{{ suite.skipped_count }}</td>
                <td>{{ "%.2f"|format(suite.duration) }}s</td>
                <td>{{ "%.1f"|format(suite.success_rate * 100) }}%</td>
            </tr>
            {% endfor %}
        </tbody>
    </table>
    
    <h2>Detailed Results</h2>
    <table>
        <thead>
            <tr>
                <th>Suite</th>
                <th>Test</th>
                <th>Status</th>
                <th>Duration</th>
                <th>Error</th>
            </tr>
        </thead>
        <tbody>
            {% for suite in test_suites %}
                {% for result in suite.results %}
                <tr>
                    <td>{{ suite.name }}</td>
                    <td>{{ result.name }}</td>
                    <td class="{{ result.status }}">{{ result.status.upper() }}</td>
                    <td>{{ "%.3f"|format(result.duration) }}s</td>
                    <td>{{ result.error or '' }}</td>
                </tr>
                {% endfor %}
            {% endfor %}
        </tbody>
    </table>
    
    {% if system_metrics %}
    <h2>System Metrics</h2>
    <div class="metric-card">
        <pre>{{ system_metrics }}</pre>
    </div>
    {% endif %}
    
    <div class="footer" style="margin-top: 40px; padding-top: 20px; border-top: 1px solid #ddd;">
        <p><em>Generated by HFT RTB Integration Test Runner</em></p>
    </div>
</body>
</html>
        """
        
        template = Template(template_str)
        
        # Calculate totals
        total_tests = sum(suite.total_count for suite in self.test_suites)
        total_passed = sum(suite.passed_count for suite in self.test_suites)
        total_failed = sum(suite.failed_count for suite in self.test_suites)
        success_rate = (total_passed / total_tests * 100) if total_tests > 0 else 0
        
        total_duration = (self.end_time - self.start_time).total_seconds() if self.start_time and self.end_time else 0
        
        # Collect system metrics
        system_metrics = {}
        for suite in self.test_suites:
            if suite.metrics:
                system_metrics.update(suite.metrics)
        
        return template.render(
            session_id=self.session_id,
            timestamp=datetime.now().strftime("%Y-%m-%d %H:%M:%S"),
            total_duration=f"{total_duration:.2f}",
            environment=os.getenv('TEST_ENV', 'default'),
            total_tests=total_tests,
            total_passed=total_passed,
            total_failed=total_failed,
            success_rate=f"{success_rate:.1f}",
            test_suites=self.test_suites,
            system_metrics=json.dumps(system_metrics, indent=2) if system_metrics else None
        )
    
    def generate_json_report(self) -> Dict[str, Any]:
        """Generate JSON report."""
        return {
            "session_id": self.session_id,
            "timestamp": datetime.now().isoformat(),
            "start_time": self.start_time.isoformat() if self.start_time else None,
            "end_time": self.end_time.isoformat() if self.end_time else None,
            "duration": (self.end_time - self.start_time).total_seconds() if self.start_time and self.end_time else 0,
            "environment": os.getenv('TEST_ENV', 'default'),
            "config": self.config,
            "test_suites": [
                {
                    "name": suite.name,
                    "test_type": suite.test_type,
                    "start_time": suite.start_time.isoformat() if suite.start_time else None,
                    "end_time": suite.end_time.isoformat() if suite.end_time else None,
                    "duration": suite.duration,
                    "total_count": suite.total_count,
                    "passed_count": suite.passed_count,
                    "failed_count": suite.failed_count,
                    "skipped_count": suite.skipped_count,
                    "error_count": suite.error_count,
                    "success_rate": suite.success_rate,
                    "metrics": suite.metrics,
                    "results": [
                        {
                            "name": result.name,
                            "status": result.status,
                            "duration": result.duration,
                            "timestamp": result.timestamp.isoformat(),
                            "details": result.details,
                            "error": result.error
                        }
                        for result in suite.results
                    ]
                }
                for suite in self.test_suites
            ]
        }
    
    async def run_tests(self, test_types: List[str], skip_services: bool = False) -> bool:
        """Run all specified tests."""
        self.start_time = datetime.now()
        
        logger.info(f"Starting integration test run: {self.session_id}")
        logger.info(f"Test types: {test_types}")
        
        # Check services unless skipped
        if not skip_services:
            if not await self.check_services():
                logger.error("Service checks failed. Use --skip-services to bypass.")
                return False
        
        # Collect initial metrics
        initial_metrics = await self.collect_system_metrics()
        
        success = True
        
        # Run tests based on types
        for test_type in test_types:
            try:
                if test_type == "rust":
                    suite = await self.run_rust_tests()
                    self.test_suites.append(suite)
                elif test_type in ["python", "all", "quick", "performance", "load", "health", "api", "kafka"]:
                    if test_type == "python":
                        test_type = "all"
                    suite = await self.run_python_tests(test_type)
                    self.test_suites.append(suite)
                else:
                    logger.warning(f"Unknown test type: {test_type}")
                    continue
                
                # Check if suite had failures
                if suite.failed_count > 0 or suite.error_count > 0:
                    success = False
                    
            except Exception as e:
                logger.error(f"Error running {test_type} tests: {e}")
                success = False
        
        # Collect final metrics
        final_metrics = await self.collect_system_metrics()
        
        self.end_time = datetime.now()
        
        # Generate reports
        await self.generate_reports()
        
        return success
    
    async def generate_reports(self):
        """Generate all reports."""
        logger.info("Generating test reports...")
        
        # HTML Report
        html_report = self.generate_html_report()
        html_file = self.reports_dir / f"integration_test_report_{self.session_id}.html"
        with open(html_file, 'w') as f:
            f.write(html_report)
        logger.info(f"HTML report: {html_file}")
        
        # JSON Report
        json_report = self.generate_json_report()
        json_file = self.reports_dir / f"integration_test_report_{self.session_id}.json"
        with open(json_file, 'w') as f:
            json.dump(json_report, f, indent=2)
        logger.info(f"JSON report: {json_file}")
        
        # Summary to console
        self.print_summary()
    
    def print_summary(self):
        """Print test summary to console."""
        print("\n" + "="*60)
        print("INTEGRATION TEST SUMMARY")
        print("="*60)
        
        total_tests = sum(suite.total_count for suite in self.test_suites)
        total_passed = sum(suite.passed_count for suite in self.test_suites)
        total_failed = sum(suite.failed_count for suite in self.test_suites)
        total_skipped = sum(suite.skipped_count for suite in self.test_suites)
        
        print(f"Session ID: {self.session_id}")
        print(f"Duration: {(self.end_time - self.start_time).total_seconds():.2f}s")
        print(f"Total Tests: {total_tests}")
        print(f"Passed: {total_passed}")
        print(f"Failed: {total_failed}")
        print(f"Skipped: {total_skipped}")
        
        if total_tests > 0:
            success_rate = total_passed / total_tests * 100
            print(f"Success Rate: {success_rate:.1f}%")
        
        print("\nTest Suites:")
        for suite in self.test_suites:
            status = "✓" if suite.failed_count == 0 and suite.error_count == 0 else "✗"
            print(f"  {status} {suite.name}: {suite.passed_count}/{suite.total_count} passed ({suite.duration:.2f}s)")
        
        print(f"\nReports generated in: {self.reports_dir}")
        print("="*60)

def main():
    """Main entry point."""
    parser = argparse.ArgumentParser(description="HFT RTB Integration Test Runner")
    parser.add_argument("test_types", nargs="*", default=["all"],
                       choices=["all", "rust", "python", "quick", "performance", "load", "health", "api", "kafka"],
                       help="Types of tests to run")
    parser.add_argument("--config", "-c", help="Path to test configuration file")
    parser.add_argument("--skip-services", action="store_true", help="Skip service availability checks")
    parser.add_argument("--env", help="Test environment (overrides TEST_ENV)")
    parser.add_argument("--verbose", "-v", action="store_true", help="Verbose output")
    
    args = parser.parse_args()
    
    if args.verbose:
        logging.getLogger().setLevel(logging.DEBUG)
    
    if args.env:
        os.environ['TEST_ENV'] = args.env
    
    # Create and run test runner
    runner = IntegrationTestRunner(args.config)
    
    try:
        success = asyncio.run(runner.run_tests(args.test_types, args.skip_services))
        sys.exit(0 if success else 1)
    except KeyboardInterrupt:
        logger.info("Test run interrupted by user")
        sys.exit(130)
    except Exception as e:
        logger.error(f"Test run failed: {e}")
        sys.exit(1)

if __name__ == "__main__":
    main()