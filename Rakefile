# frozen_string_literal: true

require 'bundler/gem_tasks'
require 'open3'
require 'rspec/core/rake_task'
require 'facter/version'

Dir.glob(File.join('tasks/**/*.rake')).each { |file| load file }

task default: :spec

desc 'Generate changelog'
task :changelog, [:version] do |_t, args|
  sh "./scripts/generate_changelog.rb #{args[:version]}"
end

namespace :pl_ci do
  desc 'build the gem and place it at the directory root'
  task :gem_build, [:gemspec] do |t, args|
    args.with_defaults(gemspec: 'facter.gemspec')
    stdout, stderr, status = Open3.capture3("gem build #{args.gemspec}")
    if !status.exitstatus.zero?
      puts "Error building facter.gemspec \n#{stdout} \n#{stderr}"
      exit(1)
    else
      puts stdout
    end
  end

  desc 'build the nightly gem and place it at the directory root'
  task :nightly_gem_build do
    # this is taken from `rake package:nightly_gem`
    extended_dot_version = %x{git describe --tags --dirty --abbrev=7}.chomp.tr('-', '.')

    require 'tempfile'
    Tempfile.create("gemspec") do |dst|
      File.open("facter.gemspec", "r") do |src|
        src.readlines.each do |line|
          dst << line.gsub(/spec\.version\s*=\s*'[0-9.]+'/, "spec.version = '#{extended_dot_version}'")
        end
      end
      dst.flush
      Rake::Task['pl_ci:gem_build'].invoke(dst)
    end
  end
end

if Rake.application.top_level_tasks.grep(/^(pl:|package:)/).any?
  begin
    require 'packaging'
    Pkg::Util::RakeUtils.load_packaging_tasks
  rescue LoadError => e
    puts "Error loading packaging rake tasks: #{e}"
  end
end
