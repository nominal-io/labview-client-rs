<?xml version='1.0' encoding='UTF-8'?>
<Project Type="Project" LVVersion="20008000">
	<Property Name="NI.LV.All.SourceOnly" Type="Bool">true</Property>
	<Property Name="NI.Project.Description" Type="Str"></Property>
	<Item Name="My Computer" Type="My Computer">
		<Property Name="NI.SortType" Type="Int">3</Property>
		<Property Name="server.app.propertiesEnabled" Type="Bool">true</Property>
		<Property Name="server.control.propertiesEnabled" Type="Bool">true</Property>
		<Property Name="server.tcp.enabled" Type="Bool">false</Property>
		<Property Name="server.tcp.port" Type="Int">0</Property>
		<Property Name="server.tcp.serviceName" Type="Str">My Computer/VI Server</Property>
		<Property Name="server.tcp.serviceName.default" Type="Str">My Computer/VI Server</Property>
		<Property Name="server.vi.callsEnabled" Type="Bool">true</Property>
		<Property Name="server.vi.propertiesEnabled" Type="Bool">true</Property>
		<Property Name="specify.custom.address" Type="Bool">false</Property>
		<Item Name="dev" Type="Folder">
			<Item Name="generate classes from ini.vi" Type="VI" URL="../dev/generate classes from ini.vi"/>
			<Item Name="scripting.vi" Type="VI" URL="../dev/scripting.vi"/>
			<Item Name="template.vi" Type="VI" URL="../dev/template.vi"/>
		</Item>
		<Item Name="tests" Type="Folder">
			<Item Name="01 - Offline client lifecycle.vi" Type="VI" URL="../tests/01 - Offline client lifecycle.vi"/>
			<Item Name="02 - Real API list + loop.vi" Type="VI" URL="../tests/02 - Real API list + loop.vi"/>
			<Item Name="03 - Staged create-verify-staged update-archive.vi" Type="VI" URL="../tests/03 - Staged create-verify-staged update-archive.vi"/>
			<Item Name="04 - Run lifecycle.vi" Type="VI" URL="../tests/04 - Run lifecycle.vi"/>
			<Item Name="05 - Dataset Lifecycle.vi" Type="VI" URL="../tests/05 - Dataset Lifecycle.vi"/>
			<Item Name="06 - Video Lifecycle.vi" Type="VI" URL="../tests/06 - Video Lifecycle.vi"/>
			<Item Name="07 - Channel metadata.vi" Type="VI" URL="../tests/07 - Channel metadata.vi"/>
			<Item Name="08 - CSV Ingest.vi" Type="VI" URL="../tests/08 - CSV Ingest.vi"/>
			<Item Name="09 - Workbook from template.vi" Type="VI" URL="../tests/09 - Workbook from template.vi"/>
			<Item Name="10 - Who am I.vi" Type="VI" URL="../tests/10 - Who am I.vi"/>
			<Item Name="11 - Workspace discovery.vi" Type="VI" URL="../tests/11 - Workspace discovery.vi"/>
			<Item Name="labview-tests.md" Type="Document" URL="../tests/labview-tests.md"/>
			<Item Name="test.csv" Type="Document" URL="../tests/test.csv"/>
		</Item>
		<Item Name="client.lvlib" Type="Library" URL="../client/client.lvlib"/>
		<Item Name="AssetAttachDatasetStaging.lvclass" Type="LVClass" URL="../client/AssetAttachDatasetStaging/AssetAttachDatasetStaging.lvclass"/>
		<Item Name="AssetCreateStaging.lvclass" Type="LVClass" URL="../client/AssetCreateStaging/AssetCreateStaging.lvclass"/>
		<Item Name="AssetUpdateStaging.lvclass" Type="LVClass" URL="../client/AssetUpdateStaging/AssetUpdateStaging.lvclass"/>
		<Item Name="Channel.lvclass" Type="LVClass" URL="../client/Channel/Channel.lvclass"/>
		<Item Name="Client.lvclass" Type="LVClass" URL="../client/Client/Client.lvclass"/>
		<Item Name="DataflashIngestStaging.lvclass" Type="LVClass" URL="../client/DataflashIngestStaging/DataflashIngestStaging.lvclass"/>
		<Item Name="Dataset.lvclass" Type="LVClass" URL="../client/Dataset/Dataset.lvclass"/>
		<Item Name="DatasetCreateStaging.lvclass" Type="LVClass" URL="../client/DatasetCreateStaging/DatasetCreateStaging.lvclass"/>
		<Item Name="DatasetUpdateStaging.lvclass" Type="LVClass" URL="../client/DatasetUpdateStaging/DatasetUpdateStaging.lvclass"/>
		<Item Name="IngestJob.lvclass" Type="LVClass" URL="../client/IngestJob/IngestJob.lvclass"/>
		<Item Name="McapIngestStaging.lvclass" Type="LVClass" URL="../client/McapIngestStaging/McapIngestStaging.lvclass"/>
		<Item Name="Run.lvclass" Type="LVClass" URL="../client/Run/Run.lvclass"/>
		<Item Name="RunCreateStaging.lvclass" Type="LVClass" URL="../client/RunCreateStaging/RunCreateStaging.lvclass"/>
		<Item Name="RunUpdateStaging.lvclass" Type="LVClass" URL="../client/RunUpdateStaging/RunUpdateStaging.lvclass"/>
		<Item Name="TabularIngestStaging.lvclass" Type="LVClass" URL="../client/TabularIngestStaging/TabularIngestStaging.lvclass"/>
		<Item Name="Template.lvclass" Type="LVClass" URL="../client/Template/Template.lvclass"/>
		<Item Name="User.lvclass" Type="LVClass" URL="../client/User/User.lvclass"/>
		<Item Name="Video.lvclass" Type="LVClass" URL="../client/Video/Video.lvclass"/>
		<Item Name="VideoCreateStaging.lvclass" Type="LVClass" URL="../client/VideoCreateStaging/VideoCreateStaging.lvclass"/>
		<Item Name="VideoUpdateStaging.lvclass" Type="LVClass" URL="../client/VideoUpdateStaging/VideoUpdateStaging.lvclass"/>
		<Item Name="Workbook.lvclass" Type="LVClass" URL="../client/Workbook/Workbook.lvclass"/>
		<Item Name="WorkbookCreateStaging.lvclass" Type="LVClass" URL="../client/WorkbookCreateStaging/WorkbookCreateStaging.lvclass"/>
		<Item Name="Workspace.lvclass" Type="LVClass" URL="../client/Workspace/Workspace.lvclass"/>
		<Item Name="Dependencies" Type="Dependencies">
			<Item Name="vi.lib" Type="Folder">
				<Item Name="Error Cluster From Error Code.vi" Type="VI" URL="/&lt;vilib&gt;/Utility/error.llb/Error Cluster From Error Code.vi"/>
				<Item Name="NI_LVConfig.lvlib" Type="Library" URL="/&lt;vilib&gt;/Utility/config.llb/NI_LVConfig.lvlib"/>
				<Item Name="Trim Whitespace.vi" Type="VI" URL="/&lt;vilib&gt;/Utility/error.llb/Trim Whitespace.vi"/>
				<Item Name="whitespace.ctl" Type="VI" URL="/&lt;vilib&gt;/Utility/error.llb/whitespace.ctl"/>
				<Item Name="Clear Errors.vi" Type="VI" URL="/&lt;vilib&gt;/Utility/error.llb/Clear Errors.vi"/>
				<Item Name="Check if File or Folder Exists.vi" Type="VI" URL="/&lt;vilib&gt;/Utility/libraryn.llb/Check if File or Folder Exists.vi"/>
				<Item Name="NI_FileType.lvlib" Type="Library" URL="/&lt;vilib&gt;/Utility/lvfile.llb/NI_FileType.lvlib"/>
				<Item Name="NI_PackedLibraryUtility.lvlib" Type="Library" URL="/&lt;vilib&gt;/Utility/LVLibp/NI_PackedLibraryUtility.lvlib"/>
				<Item Name="8.6CompatibleGlobalVar.vi" Type="VI" URL="/&lt;vilib&gt;/Utility/config.llb/8.6CompatibleGlobalVar.vi"/>
				<Item Name="Simple Error Handler.vi" Type="VI" URL="/&lt;vilib&gt;/Utility/error.llb/Simple Error Handler.vi"/>
				<Item Name="DialogType.ctl" Type="VI" URL="/&lt;vilib&gt;/Utility/error.llb/DialogType.ctl"/>
				<Item Name="General Error Handler.vi" Type="VI" URL="/&lt;vilib&gt;/Utility/error.llb/General Error Handler.vi"/>
				<Item Name="DialogTypeEnum.ctl" Type="VI" URL="/&lt;vilib&gt;/Utility/error.llb/DialogTypeEnum.ctl"/>
				<Item Name="General Error Handler Core CORE.vi" Type="VI" URL="/&lt;vilib&gt;/Utility/error.llb/General Error Handler Core CORE.vi"/>
				<Item Name="Check Special Tags.vi" Type="VI" URL="/&lt;vilib&gt;/Utility/error.llb/Check Special Tags.vi"/>
				<Item Name="TagReturnType.ctl" Type="VI" URL="/&lt;vilib&gt;/Utility/error.llb/TagReturnType.ctl"/>
				<Item Name="Set String Value.vi" Type="VI" URL="/&lt;vilib&gt;/Utility/error.llb/Set String Value.vi"/>
				<Item Name="GetRTHostConnectedProp.vi" Type="VI" URL="/&lt;vilib&gt;/Utility/error.llb/GetRTHostConnectedProp.vi"/>
				<Item Name="Error Code Database.vi" Type="VI" URL="/&lt;vilib&gt;/Utility/error.llb/Error Code Database.vi"/>
				<Item Name="Format Message String.vi" Type="VI" URL="/&lt;vilib&gt;/Utility/error.llb/Format Message String.vi"/>
				<Item Name="Find Tag.vi" Type="VI" URL="/&lt;vilib&gt;/Utility/error.llb/Find Tag.vi"/>
				<Item Name="Search and Replace Pattern.vi" Type="VI" URL="/&lt;vilib&gt;/Utility/error.llb/Search and Replace Pattern.vi"/>
				<Item Name="Set Bold Text.vi" Type="VI" URL="/&lt;vilib&gt;/Utility/error.llb/Set Bold Text.vi"/>
				<Item Name="Details Display Dialog.vi" Type="VI" URL="/&lt;vilib&gt;/Utility/error.llb/Details Display Dialog.vi"/>
				<Item Name="ErrWarn.ctl" Type="VI" URL="/&lt;vilib&gt;/Utility/error.llb/ErrWarn.ctl"/>
				<Item Name="eventvkey.ctl" Type="VI" URL="/&lt;vilib&gt;/event_ctls.llb/eventvkey.ctl"/>
				<Item Name="Not Found Dialog.vi" Type="VI" URL="/&lt;vilib&gt;/Utility/error.llb/Not Found Dialog.vi"/>
				<Item Name="Three Button Dialog.vi" Type="VI" URL="/&lt;vilib&gt;/Utility/error.llb/Three Button Dialog.vi"/>
				<Item Name="Three Button Dialog CORE.vi" Type="VI" URL="/&lt;vilib&gt;/Utility/error.llb/Three Button Dialog CORE.vi"/>
				<Item Name="LVRectTypeDef.ctl" Type="VI" URL="/&lt;vilib&gt;/Utility/miscctls.llb/LVRectTypeDef.ctl"/>
				<Item Name="Longest Line Length in Pixels.vi" Type="VI" URL="/&lt;vilib&gt;/Utility/error.llb/Longest Line Length in Pixels.vi"/>
				<Item Name="Convert property node font to graphics font.vi" Type="VI" URL="/&lt;vilib&gt;/Utility/error.llb/Convert property node font to graphics font.vi"/>
				<Item Name="Get Text Rect.vi" Type="VI" URL="/&lt;vilib&gt;/picture/picture.llb/Get Text Rect.vi"/>
				<Item Name="Get String Text Bounds.vi" Type="VI" URL="/&lt;vilib&gt;/Utility/error.llb/Get String Text Bounds.vi"/>
				<Item Name="LVBoundsTypeDef.ctl" Type="VI" URL="/&lt;vilib&gt;/Utility/miscctls.llb/LVBoundsTypeDef.ctl"/>
				<Item Name="BuildHelpPath.vi" Type="VI" URL="/&lt;vilib&gt;/Utility/error.llb/BuildHelpPath.vi"/>
				<Item Name="GetHelpDir.vi" Type="VI" URL="/&lt;vilib&gt;/Utility/error.llb/GetHelpDir.vi"/>
				<Item Name="TRef TravTarget.ctl" Type="VI" URL="/&lt;vilib&gt;/Utility/traverseref.llb/TRef TravTarget.ctl"/>
				<Item Name="Application Directory.vi" Type="VI" URL="/&lt;vilib&gt;/Utility/file.llb/Application Directory.vi"/>
				<Item Name="VI Scripting - Traverse.lvlib" Type="Library" URL="/&lt;vilib&gt;/Utility/traverseref.llb/VI Scripting - Traverse.lvlib"/>
				<Item Name="TRef Traverse.vi" Type="VI" URL="/&lt;vilib&gt;/Utility/traverseref.llb/TRef Traverse.vi"/>
				<Item Name="Write Delimited Spreadsheet.vi" Type="VI" URL="/&lt;vilib&gt;/Utility/file.llb/Write Delimited Spreadsheet.vi"/>
				<Item Name="Write Delimited Spreadsheet (DBL).vi" Type="VI" URL="/&lt;vilib&gt;/Utility/file.llb/Write Delimited Spreadsheet (DBL).vi"/>
				<Item Name="Write Spreadsheet String.vi" Type="VI" URL="/&lt;vilib&gt;/Utility/file.llb/Write Spreadsheet String.vi"/>
				<Item Name="Write Delimited Spreadsheet (I64).vi" Type="VI" URL="/&lt;vilib&gt;/Utility/file.llb/Write Delimited Spreadsheet (I64).vi"/>
				<Item Name="Write Delimited Spreadsheet (string).vi" Type="VI" URL="/&lt;vilib&gt;/Utility/file.llb/Write Delimited Spreadsheet (string).vi"/>
				<Item Name="LVPointTypeDef.ctl" Type="VI" URL="/&lt;vilib&gt;/Utility/miscctls.llb/LVPointTypeDef.ctl"/>
			</Item>
			<Item Name="nominalClient_64.dll" Type="Document" URL="../bin/nominalClient_64.dll"/>
		</Item>
		<Item Name="Build Specifications" Type="Build"/>
	</Item>
</Project>
