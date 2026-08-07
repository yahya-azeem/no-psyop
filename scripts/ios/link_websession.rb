#!/usr/bin/env ruby
# Adds ios/WebSession/WebSessionHost.swift to the Tauri-generated iOS target.
# Runs on the macOS CI runner after `bun run tauri ios init`:
#
#   bundle install (xcodeproj gem) && ruby scripts/ios/link_websession.rb
#
# This is the only step that mutates the generated project; the JS engine is
# embedded as a bundle resource via tauri.conf.json -> bundle.resources, so it
# needs no pbxproj changes.
require 'xcodeproj'

ROOT = File.expand_path('../..', __dir__)
APPLE = File.join(ROOT, 'src-tauri', 'gen', 'apple')
SWIFT = File.join(ROOT, 'ios', 'WebSession', 'WebSessionHost.swift')

project_path = Dir.glob(File.join(APPLE, '**', '*.xcodeproj')).first
abort "no .xcodeproj under #{APPLE} — run `bun run tauri ios init` first" unless project_path

project = Xcodeproj::Project.open(project_path)
targets = project.targets.select { |t| t.respond_to?(:source_build_phase) }
abort 'no app target found' if targets.empty?

targets.each do |target|
  next if target.source_build_phase.files_references.any? { |f| f.display_name == 'WebSessionHost.swift' }

  group = project.main_group.find_subpath('WebSession', true)
  file_ref = group.new_file(SWIFT)
  target.source_build_phase.add_file_reference(file_ref)
  puts "linked WebSessionHost.swift -> #{target.name}"
end

project.save
puts "updated #{project_path}"